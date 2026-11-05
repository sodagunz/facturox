pub fn money(cents: i64) -> String {
    let magnitude = cents.unsigned_abs();
    format!(
        "{}$ {}.{:02}",
        if cents < 0 { "−" } else { "" },
        magnitude / 100,
        magnitude % 100
    )
}
