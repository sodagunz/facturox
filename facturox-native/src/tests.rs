use crate::{
    Draft,
    messages::Message,
    state::{App, Field},
    update::{pesos, total},
};
use facturox_core::{
    backend::{Error, Receipt, Request},
    domain::{Command, Product},
};
fn pending() -> App {
    App {
        pending: Some(Request {
            operation_id: "operation".into(),

            command: Command::CreateOrder {
                id: "order".into(),
                lines: vec![],
            },
        }),
        active: Some(1),
        ..App::default()
    }
}
#[test]
fn catalog_search_is_local_independent_and_preserves_the_draft() {
    let product = Product {
        id: "p".into(),
        name: "Café".into(),
        barcode: Some("001234".into()),
        price: 100,
    };
    let mut app = App {
        inventory_search: "other inventory query".into(),
        ..App::default()
    };
    app.draft.add(&product).unwrap();
    app.inventory.products.insert(product.id.clone(), product);
    let before = app.draft.lines.clone();
    assert_eq!(
        app.update(Message::Edit(Field::CatalogSearch, "CAFE 001".into()))
            .units(),
        0
    );
    assert_eq!(app.catalog_search, "CAFE 001");
    assert_eq!(app.inventory_search, "other inventory query");
    assert_eq!(app.draft.lines, before);
    assert_eq!(
        crate::product_search::filter(app.inventory.products.values(), &app.catalog_search).len(),
        1
    );
    assert_eq!(
        app.update(Message::Edit(Field::CatalogSearch, "no match".into()))
            .units(),
        0
    );
    assert_eq!(
        crate::product_search::filter(app.inventory.products.values(), &app.catalog_search),
        Vec::<&Product>::new()
    );
    assert_eq!(app.draft.lines, before);
    assert!(app.pending.is_none());
    assert!(app.active.is_none());
}

#[test]
fn inventory_search_is_local_and_preserves_other_editing_state() {
    let mut app = App {
        selected: Some("p".into()),
        delta: "2".into(),
        name: "Unsaved product".into(),
        ..App::default()
    };
    let task = app.update(Message::Edit(Field::InventorySearch, "cafe 001".into()));
    assert_eq!(task.units(), 0);
    assert_eq!(app.inventory_search, "cafe 001");
    assert_eq!(app.selected.as_deref(), Some("p"));
    assert_eq!(app.delta, "2");
    assert_eq!(app.name, "Unsaved product");
    assert!(app.pending.is_none());
    assert!(app.active.is_none());
    assert_eq!(app.inventory.order, 0);
}

#[test]
fn selecting_another_product_clears_the_previous_adjustment() {
    let mut app = App {
        selected: Some("first".into()),
        delta: "-2".into(),
        reason: "Damaged".into(),
        ..App::default()
    };
    assert_eq!(app.update(Message::Select("second".into())).units(), 0);
    assert_eq!(app.selected.as_deref(), Some("second"));
    assert_eq!(app.delta, "");
    assert_eq!(app.reason, "");
}

#[test]
fn committed_batch_updates_frontend_once_without_reloading() {
    use facturox_core::domain::{EventEnvelope, Inventory};
    let command = Command::CreateProduct {
        product: Product {
            id: "p".into(),
            name: "Tea".into(),
            barcode: None,
            price: 100,
        },
        opening_quantity: 3,
    };
    let events = Inventory::default().decide(&command, 0).unwrap();
    let envelopes = events
        .iter()
        .enumerate()
        .map(|(index, event)| EventEnvelope {
            order: i64::try_from(index).unwrap() + 1,
            operation_id: "operation".into(),
            occurred_at: 0,
            event: event.clone(),
        })
        .collect();
    let receipt = Receipt {
        order: 2,
        events,
        envelopes,
    };
    let mut app = pending();
    app.pending.as_mut().unwrap().command = command;
    app.name = "Tea".into();
    assert_eq!(
        app.update(Message::Completed(
            1,
            "operation".into(),
            Ok(receipt.clone())
        ))
        .units(),
        0
    );
    assert_eq!(app.inventory.order, 2);
    assert_eq!(
        app.inventory.quantities.get("p").map(|q| q.quantity),
        Some(3)
    );
    assert!(app.pending.is_none());
    assert!(app.active.is_none());
    assert!(!app.stale);
    assert_eq!(app.name, "");
    assert_eq!(
        app.update(Message::Completed(1, "operation".into(), Ok(receipt)))
            .units(),
        0
    );
    assert_eq!(app.inventory.order, 2);
}

#[tokio::test]
async fn uncertain_committed_response_retries_identical_request_exactly_once() {
    use crate::update::execute_with_recovery;
    use facturox_core::{backend::Backend, domain::OrderLine};
    let directory = tempfile::tempdir().unwrap();
    let backend = Backend::open(directory.path().join("test.sqlite"))
        .await
        .unwrap();
    backend
        .execute(Request {
            operation_id: "product".into(),
            command: Command::CreateProduct {
                product: Product {
                    id: "p".into(),
                    name: "Tea".into(),
                    barcode: None,
                    price: 100,
                },
                opening_quantity: 3,
            },
        })
        .await
        .unwrap();
    let request = Request {
        operation_id: "checkout".into(),
        command: Command::CreateOrder {
            id: "order".into(),
            lines: vec![OrderLine {
                product_id: "p".into(),
                name: "Tea".into(),
                unit_price: 100,
                quantity: 1,
            }],
        },
    };
    let mut calls = 0;
    let receipt = execute_with_recovery(request.clone(), |actual| {
        assert_eq!(actual, request);
        calls += 1;
        let lose_response = calls == 1;
        let backend = backend.clone();
        async move {
            let receipt = backend.execute(actual).await?;
            if lose_response {
                Err(Error::Uncertain("response lost after commit".into()))
            } else {
                Ok(receipt)
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(calls, 2);
    let snapshot = backend.snapshot().await.unwrap();
    assert_eq!(snapshot.orders.len(), 1);
    assert_eq!(snapshot.quantities.get("p").unwrap().quantity, 2);
    assert_eq!(snapshot.order, receipt.order);
}

#[tokio::test]
async fn recovery_is_bounded_and_does_not_retry_definite_results() {
    use crate::update::execute_with_recovery;
    for (result, expected_calls) in [
        (Err(Error::Uncertain("disk unavailable".into())), 2),
        (Err(Error::Rejected("invalid".into())), 1),
        (
            Ok(Receipt {
                order: 0,
                events: vec![],
                envelopes: vec![],
            }),
            1,
        ),
    ] {
        let request = pending().pending.unwrap();
        let mut calls = 0;
        let actual = execute_with_recovery(request.clone(), |actual| {
            assert_eq!(actual, request);
            calls += 1;
            std::future::ready(result.clone())
        })
        .await;
        assert_eq!(actual, result);
        assert_eq!(calls, expected_calls);
    }
}

#[tokio::test]
async fn receipt_gap_automatically_synchronizes_and_never_restores_confirmed_draft() {
    use facturox_core::backend::Backend;
    let directory = tempfile::tempdir().unwrap();
    let backend = Backend::open(directory.path().join("test.sqlite"))
        .await
        .unwrap();
    let mut app = pending();
    app.backend = Some(backend.clone());
    let product = Product {
        id: "p".into(),
        name: "Tea".into(),
        barcode: None,
        price: 100,
    };
    backend
        .execute(Request {
            operation_id: "product".into(),
            command: Command::CreateProduct {
                product: product.clone(),
                opening_quantity: 3,
            },
        })
        .await
        .unwrap();
    app.draft.add(&product).unwrap();
    let request = app.pending.as_mut().unwrap();
    request.command = Command::CreateOrder {
        id: "order".into(),
        lines: app.draft.lines.clone(),
    };
    let receipt = backend.execute(request.clone()).await.unwrap();
    // The frontend missed product creation, so the order receipt alone cannot apply.
    assert_eq!(
        app.update(Message::Completed(1, "operation".into(), Ok(receipt)))
            .units(),
        1
    );
    assert_eq!(
        app.draft.lines,
        Vec::<facturox_core::domain::OrderLine>::new()
    );
    assert!(app.pending.is_none());
    assert!(app.stale);
    assert!(!app.editable());
    let token = app.active.unwrap();
    assert_eq!(app.update(Message::Checkout).units(), 0);
    let inventory = backend.snapshot().await.unwrap();
    let _ = app.update(Message::Synchronized(token, Ok(inventory)));
    assert!(app.editable());
    assert_eq!(app.error, "");
    assert_eq!(
        app.draft.lines,
        Vec::<facturox_core::domain::OrderLine>::new()
    );
    assert_eq!(app.inventory.orders.len(), 1);
    assert_eq!(app.inventory.quantities.get("p").unwrap().quantity, 2);
}

#[test]
fn synchronization_failure_keeps_confirmed_operation_locked() {
    let mut app = App {
        stale: true,
        active: Some(2),
        ..App::default()
    };
    let _ = app.update(Message::Synchronized(
        2,
        Err(Error::Uncertain("disk".into())),
    ));
    assert!(app.stale);
    assert!(app.active.is_none());
    assert!(app.pending.is_none());
    assert_eq!(
        app.draft.lines,
        Vec::<facturox_core::domain::OrderLine>::new()
    );
    assert!(app.error.contains("Changes remain locked"));
    let error = app.error.clone();
    for message in [
        Message::Checkout,
        Message::Create,
        Message::Adjust,
        Message::Edit(Field::Name, "changed".into()),
    ] {
        assert_eq!(app.update(message).units(), 0);
    }
    assert_eq!(app.name, "");
    assert_eq!(app.error, error);
}

#[test]
fn recovery_controls_and_manual_messages_are_absent() {
    let view = include_str!("view.rs");
    let messages = include_str!("messages.rs");
    for label in [
        "Refresh",
        "Retry",
        "Local inventory · order",
        "Message::Open",
    ] {
        assert!(!view.contains(label), "unexpected recovery UI: {label}");
    }
    for variant in ["Refresh", "Retry"] {
        assert!(
            !messages.contains(variant),
            "unexpected manual message: {variant}"
        );
    }
}

#[test]
fn decimal_pesos_are_exact_and_checked() {
    assert_eq!(pesos("1250.50"), Ok(125_050));
    assert_eq!(pesos("12,5"), Ok(1250));
    assert_eq!(pesos("0"), Ok(0));
    for invalid in ["-1", "1.234", "1e3", "9223372036854775807", ""] {
        assert!(pesos(invalid).is_err());
    }
}
#[test]
fn draft_arithmetic_checks_total_overflow() {
    let product = Product {
        id: "p".into(),
        name: "Product".into(),
        barcode: None,
        price: i64::MAX,
    };
    let mut draft = Draft::default();
    assert!(draft.add(&product).is_ok());
    // The frontend checks the candidate total before publishing the edit.
    let mut candidate = draft.clone();
    let added = candidate.add(&product);
    assert!(added.is_err() || total(&candidate).is_err());
    assert_eq!(total(&draft), Ok(i64::MAX));
}
#[test]
fn uncertain_result_freezes_exact_request_and_local_edits_do_not_schedule_work() {
    let mut app = pending();
    let request = app.pending.clone();
    let task = app.update(Message::Completed(
        1,
        "operation".into(),
        Err(Error::Uncertain("disk".into())),
    ));
    assert_eq!(task.units(), 0);
    let task = app.update(Message::Edit(Field::Name, "changed".into()));
    assert_eq!(task.units(), 0);
    assert_eq!(app.name, "");
    assert_eq!(app.pending, request);
    assert!(app.error.contains("Inspect recorded history"));
    assert!(!app.editable());
    for message in [
        Message::Checkout,
        Message::Create,
        Message::Adjust,
        Message::Discard,
    ] {
        assert_eq!(app.update(message).units(), 0);
        assert_eq!(app.pending, request);
    }
}
#[test]
fn successful_stale_receipt_without_backend_clears_draft_and_stays_locked() {
    let mut app = pending();
    app.draft.lines.push(facturox_core::domain::OrderLine {
        product_id: "p".into(),
        name: "Product".into(),
        unit_price: 100,
        quantity: 1,
    });
    let receipt = Receipt {
        order: 9,
        events: vec![],
        envelopes: vec![],
    };
    let _ = app.update(Message::Completed(
        1,
        "operation".into(),
        Ok(receipt.clone()),
    ));
    assert_eq!(
        app.draft.lines,
        Vec::<facturox_core::domain::OrderLine>::new()
    );
    assert!(app.pending.is_none());
    assert!(app.stale);
    assert!(!app.editable());
    assert_ne!(app.error, "");
    let error = app.error.clone();
    let _ = app.update(Message::Completed(1, "operation".into(), Ok(receipt)));
    assert_eq!(app.error, error);
}
#[test]
fn business_rejection_preserves_draft_and_company_save_errors_preserve_form() {
    let mut app = pending();
    app.draft.lines.push(facturox_core::domain::OrderLine {
        product_id: "p".into(),
        name: "Tea".into(),
        unit_price: 100,
        quantity: 2,
    });
    let lines = app.draft.lines.clone();
    let _ = app.update(Message::Completed(
        1,
        "operation".into(),
        Err(Error::Rejected("Insufficient quantity".into())),
    ));
    assert_eq!(app.draft.lines, lines);
    assert!(!app.stale);
    assert!(app.pending.is_none());
    app.company.business_name = "Unsaved business".into();
    app.active = Some(2);
    let _ = app.update(Message::CompanySaved(
        2,
        Err(Error::Rejected("invalid".into())),
    ));
    assert_eq!(app.company.business_name, "Unsaved business");
}

#[test]
fn local_edits_and_unit_changes_schedule_no_tasks() {
    let mut app = App::default();
    assert_eq!(
        app.update(Message::Edit(Field::Price, "12.50".into()))
            .units(),
        0
    );
    assert_eq!(app.price, "12.50");
    let product = Product {
        id: "p".into(),
        name: "Product".into(),
        barcode: None,
        price: 100,
    };
    app.inventory.products.insert(product.id.clone(), product);
    assert_eq!(app.update(Message::Add("p".into())).units(), 0);
    assert_eq!(app.update(Message::Add("p".into())).units(), 0);
    assert_eq!(total(&app.draft), Ok(200));
    assert_eq!(app.update(Message::Minus("p".into())).units(), 0);
    assert_eq!(total(&app.draft), Ok(100));
    let _ = app.update(Message::Minus("p".into()));
    assert_eq!(app.draft.lines.len(), 0);
    let _ = app.update(Message::Add("p".into()));
    if let Some(line) = app.draft.lines.first_mut() {
        line.quantity = i64::MAX;
        line.unit_price = 0;
    }
    let _ = app.update(Message::Add("p".into()));
    assert_eq!(
        app.draft.lines.first().map(|line| line.quantity),
        Some(i64::MAX)
    );
    assert_ne!(app.error, "");
}
