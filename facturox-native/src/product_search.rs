//! Pure catalog search and advisory duplicate suggestions; no persistence or UI state.

use facturox_core::domain::Product;
use fuzzy_matcher::{FuzzyMatcher, skim::SkimMatcherV2};
use strsim::normalized_damerau_levenshtein;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

const MAX_FUZZY_CHARS: usize = 200;
const MIN_SIMILARITY: f64 = 0.8;

fn normalize(value: &str) -> String {
    value
        .nfd()
        .filter(|character| !is_combining_mark(*character))
        .flat_map(char::to_lowercase)
        .map(|character| {
            if character.is_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn by_name_id(left: &Product, right: &Product) -> std::cmp::Ordering {
    left.name.cmp(&right.name).then(left.id.cmp(&right.id))
}

/// Fuzzy-match every normalized query token against either the name or barcode.
/// Characters must occur in order but need not be adjacent. Tokens may appear in
/// any order; exact and compact matches rank first, with stable name/ID ties.
/// Barcodes remain strings, preserving zeroes.
/// Empty queries return everything, ordered by stored name and then ID.
#[must_use]
pub fn filter<'a>(products: impl Iterator<Item = &'a Product>, query: &str) -> Vec<&'a Product> {
    let query = normalize(query);
    let tokens: Vec<_> = query.split_whitespace().collect();
    let engine = SkimMatcherV2::default();
    let mut matches: Vec<_> = products
        .filter_map(|product| {
            let name = normalize(&product.name);
            let barcode = normalize(product.barcode.as_deref().unwrap_or_default());
            let score = tokens.iter().try_fold(0_i64, |score, token| {
                let best = engine
                    .fuzzy_match(&name, token)
                    .max(engine.fuzzy_match(&barcode, token))?;
                score.checked_add(best)
            })?;
            let exact = !query.is_empty() && (query == name || query == barcode);
            Some((product, exact, score))
        })
        .collect();
    matches.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then(right.2.cmp(&left.2))
            .then_with(|| by_name_id(left.0, right.0))
    });
    matches.into_iter().map(|entry| entry.0).collect()
}

fn fuzzy_score(query: &str, candidate: &str) -> f64 {
    // Bound distance calculations by Unicode scalar count rather than byte slicing.
    // Overlong names still support exact matches.
    if query.chars().count() > MAX_FUZZY_CHARS || candidate.chars().count() > MAX_FUZZY_CHARS {
        return 0.0;
    }
    let whole = normalized_damerau_levenshtein(query, candidate);
    let mut token_score: f64 = 1.0;
    for token in query.split_whitespace() {
        let best = candidate
            .split_whitespace()
            .map(|other| {
                if token == other || (token.chars().count() >= 3 && other.starts_with(token)) {
                    1.0
                } else if token.chars().count() >= 3 && other.chars().count() >= 3 {
                    normalized_damerau_levenshtein(token, other)
                } else {
                    0.0
                }
            })
            .fold(0.0, f64::max);
        token_score = token_score.min(best);
    }
    whole.max(token_score)
}

/// Return up to five duplicate warnings, not definitive duplicate decisions.
/// Exact (trimmed, case-sensitive) barcodes outrank normalized exact names.
/// Otherwise require 80% whole-name similarity or 80% for every query token
/// against a name token; prefixes of at least three characters also qualify.
/// Blank or fewer-than-three-character names receive only exact matches.
/// Fuzzy work is limited to names of at most 200 normalized characters.
/// Ties are deterministic by stored name, then ID. Barcodes are never fuzzy.
#[must_use]
pub fn similar<'a>(
    products: impl Iterator<Item = &'a Product>,
    name: &str,
    barcode: &str,
) -> Vec<&'a Product> {
    let name = normalize(name);
    let barcode = barcode.trim();
    let fuzzy_allowed = name.chars().filter(|c| !c.is_whitespace()).count() >= 3;
    let mut matches: Vec<_> = products
        .filter_map(|product| {
            let candidate = normalize(&product.name);
            let (priority, score) = if !barcode.is_empty()
                && product
                    .barcode
                    .as_deref()
                    .is_some_and(|stored| stored.trim() == barcode)
            {
                (3, 1.0)
            } else if !name.is_empty() && name == candidate {
                (2, 1.0)
            } else if fuzzy_allowed && !candidate.is_empty() {
                let score = fuzzy_score(&name, &candidate);
                if score < MIN_SIMILARITY {
                    return None;
                }
                (1, score)
            } else {
                return None;
            };
            Some((product, priority, score))
        })
        .collect();
    matches.sort_by(|left, right| {
        right
            .1
            .cmp(&left.1)
            .then(right.2.total_cmp(&left.2))
            .then_with(|| by_name_id(left.0, right.0))
    });
    matches.into_iter().take(5).map(|entry| entry.0).collect()
}

#[cfg(test)]
mod tests {
    use super::{filter, similar};
    use facturox_core::domain::Product;

    fn product(id: &str, name: &str, barcode: Option<&str>) -> Product {
        Product {
            id: id.into(),
            name: name.into(),
            barcode: barcode.map(str::to_owned),
            price: 100,
        }
    }
    fn ids(products: Vec<&Product>) -> Vec<&str> {
        products
            .into_iter()
            .map(|product| product.id.as_str())
            .collect()
    }

    #[test]
    fn filter_matches_noncontiguous_characters_in_order() {
        let products = [
            product("wine", "Jurancon sec", None),
            product("other", "Jurancon brut", None),
        ];
        assert_eq!(ids(filter(products.iter(), "aos")), ["wine"]);
        assert_eq!(ids(filter(products.iter(), "SEC jur")), ["wine"]);
        assert_eq!(filter(products.iter(), "soa"), Vec::<&Product>::new());
    }

    #[test]
    fn filter_ranks_exact_and_compact_matches_before_scattered_matches() {
        let products = [
            product("scattered", "Jurancon sec", None),
            product("compact", "Chaos", None),
            product("exact", "AOS", None),
        ];
        assert_eq!(
            ids(filter(products.iter(), "aos")),
            ["exact", "compact", "scattered"]
        );
        assert_eq!(
            ids(filter(products.iter().rev(), "aos")),
            ["exact", "compact", "scattered"]
        );
    }

    #[test]
    fn filter_normalizes_accents_case_punctuation_and_token_order() {
        let products = [product("a", "Café-Yerba", Some("001234"))];
        for query in ["CAFE yerba", "YERBA cafe", "cafe\u{301}", "yerba 0012"] {
            assert_eq!(ids(filter(products.iter(), query)), ["a"]);
        }
        for query in ["coffee", "001243", "0001234"] {
            assert_eq!(filter(products.iter(), query), Vec::<&Product>::new());
        }
    }
    #[test]
    fn empty_filter_is_sorted_by_name_then_id() {
        let products = [
            product("z", "Tea", None),
            product("b", "Coffee", None),
            product("a", "Coffee", None),
        ];
        assert_eq!(ids(filter(products.iter(), "  ")), ["a", "b", "z"]);
    }
    #[test]
    fn exact_name_precedes_typos_and_partial_names() {
        let products = [
            product("partial", "Coffee beans", None),
            product("typo", "Coffe", None),
            product("exact", "CÓFFEE", None),
        ];
        assert_eq!(
            ids(similar(products.iter(), "coffee", "")),
            ["exact", "partial", "typo"]
        );
    }
    #[test]
    fn typos_transpositions_and_partial_multiword_names_match() {
        let products = [product("a", "Coffee roasted beans", None)];
        for name in ["Coffe", "Cofefe", "beans coffee", "coff roast"] {
            assert_eq!(ids(similar(products.iter(), name, "")), ["a"]);
        }
        for name in ["Coffee detergent", "banana"] {
            assert_eq!(similar(products.iter(), name, ""), Vec::<&Product>::new());
        }
    }
    #[test]
    fn blank_and_short_names_only_allow_exact_matches() {
        let products = [product("a", "Té", None), product("b", "Tea", None)];
        for name in ["", " ", "---", "t"] {
            assert_eq!(similar(products.iter(), name, ""), Vec::<&Product>::new());
        }
        assert_eq!(ids(similar(products.iter(), "TE", "")), ["a"]);
    }
    #[test]
    fn exact_barcode_wins_even_with_blank_or_unrelated_name() {
        let products = [
            product("name", "Coffee", Some("001235")),
            product("barcode", "Banana", Some("001234")),
        ];
        assert_eq!(
            ids(similar(products.iter(), "coffee", "001234")),
            ["barcode", "name"]
        );
        assert_eq!(ids(similar(products.iter(), "", " 001234 ")), ["barcode"]);
        for barcode in ["001243", "1234"] {
            assert_eq!(
                similar(products.iter(), "", barcode),
                Vec::<&Product>::new()
            );
        }
    }
    #[test]
    fn top_five_are_stable_independent_of_iterator_order() {
        let products: Vec<_> = ["f", "d", "b", "a", "e", "c"]
            .into_iter()
            .map(|id| product(id, "Coffee", None))
            .collect();
        let expected = ["a", "b", "c", "d", "e"];
        assert_eq!(ids(similar(products.iter(), "coffee", "")), expected);
        assert_eq!(ids(similar(products.iter().rev(), "coffee", "")), expected);
    }
    #[test]
    fn unicode_and_overlong_names_are_safe_and_keep_exact_matches() {
        let long = "é".repeat(201);
        let products = [
            product("long", &long, None),
            product("unicode", "茶葉 Café 🫖", None),
        ];
        assert_eq!(ids(similar(products.iter(), &long, "")), ["long"]);
        assert_eq!(
            similar(products.iter(), &format!("{long}x"), ""),
            Vec::<&Product>::new()
        );
        assert_eq!(ids(filter(products.iter(), "茶葉 CAFE")), ["unicode"]);
        assert_eq!(similar(products.iter(), "🫖", ""), Vec::<&Product>::new());
        assert_eq!(ids(similar(products.iter(), "茶葉 Café", "")), ["unicode"]);
    }
}
