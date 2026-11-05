#![allow(
    clippy::unwrap_used,
    reason = "Test failures should identify the failing assertion or setup operation."
)]
use crate::{
    backend::{Backend, Error, Request},
    domain::{Command, Event, Inventory, Order, OrderLine, Product},
};
use sqlx::Connection;
fn product() -> Product {
    Product {
        id: "p".into(),
        name: "Tea".into(),
        barcode: None,
        price: 100,
    }
}
fn create() -> Command {
    Command::CreateProduct {
        product: product(),
        opening_quantity: 3,
    }
}
fn request(id: &str, command: Command) -> Request {
    Request {
        operation_id: id.into(),
        command,
    }
}
fn order() -> Command {
    Command::CreateOrder {
        id: "order".into(),
        lines: vec![OrderLine {
            product_id: "p".into(),
            name: "Tea".into(),
            unit_price: 100,
            quantity: 1,
        }],
    }
}
#[test]
fn domain_batches_replay_and_invalid_commands_are_pure() {
    let mut inventory = Inventory::default();
    let batch = inventory.decide(&create(), 0).unwrap();
    assert_eq!(batch.len(), 2);
    for event in batch {
        inventory.apply(&event).unwrap();
    }
    assert_eq!(inventory.order, 2);
    let before = inventory.clone();
    for delta in [0, -4, i64::MAX, i64::MIN] {
        assert!(
            Command::AdjustProductQuantity {
                product_id: "p".into(),
                delta,
                reason: None
            }
            .validate(&inventory)
            .is_err()
        );
        assert_eq!(inventory, before);
    }
    let mut invalid = product();
    invalid.id = "other".into();
    assert!(
        Command::CreateProduct {
            product: invalid,
            opening_quantity: 0
        }
        .validate(&inventory)
        .is_err()
    );
    let events = inventory.decide(&order(), 0).unwrap();
    for e in events {
        inventory.apply(&e).unwrap();
    }
    assert_eq!(inventory.quantities.get("p").map(|q| q.quantity), Some(2));
    assert_eq!(inventory.orders.get("order").unwrap().total().unwrap(), 100);
    let mut replay = Inventory::default();
    for event in before
        .decide(
            &Command::AdjustProductQuantity {
                product_id: "p".into(),
                delta: 1,
                reason: None,
            },
            0,
        )
        .unwrap()
    {
        assert!(replay.apply(&event).is_err());
    }
}
#[test]
fn quotes_and_amounts_are_validated_without_repricing() {
    let mut inventory = Inventory::default();
    for e in inventory.decide(&create(), 0).unwrap() {
        inventory.apply(&e).unwrap();
    }
    let mut lines = vec![OrderLine {
        product_id: "p".into(),
        name: "Tea".into(),
        unit_price: 100,
        quantity: 2,
    }];
    lines.first_mut().unwrap().unit_price = 1;
    assert!(
        inventory
            .decide(
                &Command::CreateOrder {
                    id: "o".into(),
                    lines: lines.clone()
                },
                0
            )
            .is_err()
    );
    lines.first_mut().unwrap().unit_price = 100;
    lines.push(lines.first().unwrap().clone());
    assert!(
        inventory
            .decide(
                &Command::CreateOrder {
                    id: "o".into(),
                    lines
                },
                0
            )
            .is_err()
    );
    assert!(
        inventory
            .decide(
                &Command::CreateOrder {
                    id: "o".into(),
                    lines: vec![]
                },
                0
            )
            .is_err()
    );
    let mut expensive = Inventory::default();
    let mut p = product();
    p.price = i64::MAX;
    for e in expensive
        .decide(
            &Command::CreateProduct {
                product: p.clone(),
                opening_quantity: 3,
            },
            0,
        )
        .unwrap()
    {
        expensive.apply(&e).unwrap();
    }
    assert!(
        expensive
            .decide(
                &Command::CreateOrder {
                    id: "o".into(),
                    lines: vec![OrderLine {
                        product_id: p.id,
                        name: p.name,
                        unit_price: p.price,
                        quantity: 2
                    }]
                },
                0
            )
            .is_err()
    );
}
#[tokio::test]
async fn durable_exact_retry_reopen_dedup_and_rebuild() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let backend = Backend::open(&path).await.unwrap();
    let req = request("create", create());
    let receipt = backend.execute(req.clone()).await.unwrap();
    assert_eq!(receipt.events.len(), 2);
    assert_eq!(backend.execute(req.clone()).await.unwrap(), receipt);
    let conflicting = request("create", order());
    assert!(matches!(
        backend.execute(conflicting).await,
        Err(Error::Rejected(_))
    ));

    backend.execute(request("order", order())).await.unwrap();
    backend.set_dark_theme(true).await.unwrap();
    let before = backend.snapshot().await.unwrap();
    {
        let mut c = tokio::sync::MutexGuard::map(backend.state.lock().await, |s| &mut s.connection);
        sqlx::raw_sql("DELETE FROM products; DELETE FROM orders;")
            .execute(&mut *c)
            .await
            .unwrap();
    }
    backend.rebuild_projections().await.unwrap();
    backend.rebuild_projections().await.unwrap();
    assert_eq!(backend.snapshot().await.unwrap(), before);
    drop(backend);
    let reopened = Backend::open(&path).await.unwrap();
    assert_eq!(reopened.execute(req).await.unwrap(), receipt);
    assert!(reopened.dark_theme().await.unwrap());
    let mut c = tokio::sync::MutexGuard::map(reopened.state.lock().await, |s| &mut s.connection);
    let quantity = sqlx::query_scalar!("SELECT quantity FROM products WHERE id='p'")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    assert_eq!(quantity, 2);
    let count = sqlx::query_scalar!("SELECT count(*) FROM orders")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let events = sqlx::query!(
        "SELECT typeof(payload) AS \"storage!: String\", json_valid(payload,8) AS \"valid!: i64\", json_extract(payload,'$.ProductCreated.name') AS \"name: String\" FROM inventory_events ORDER BY \"order\""
    )
    .fetch_all(&mut *c)
    .await
    .unwrap();
    assert_eq!(events.len(), 3);
    assert!(
        events
            .iter()
            .all(|row| row.storage == "blob" && row.valid == 1)
    );
    assert_eq!(events.first().unwrap().name.as_deref(), Some("Tea"));
    let projected = sqlx::query!(
        "SELECT typeof(payload) AS \"storage!: String\", json_valid(payload,8) AS \"valid!: i64\", json_extract(payload,'$.id') AS \"id!: String\", json(payload) AS \"payload!: String\" FROM orders"
    )
    .fetch_one(&mut *c)
    .await
    .unwrap();
    assert_eq!(projected.storage, "blob");
    assert_eq!(projected.valid, 1);
    assert_eq!(projected.id, "order");
    assert_eq!(
        serde_json::from_str::<Order>(&projected.payload).unwrap(),
        before.orders["order"]
    );
}
#[tokio::test]
async fn independent_writers_cannot_oversell() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let a = Backend::open(&path).await.unwrap();
    let b = Backend::open(&path).await.unwrap();
    a.execute(request(
        "create",
        Command::CreateProduct {
            product: product(),
            opening_quantity: 1,
        },
    ))
    .await
    .unwrap();
    let ra = request("a", order());
    let mut rb = request("b", order());
    if let Command::CreateOrder { id, .. } = &mut rb.command {
        *id = "second".into();
    }
    let (first, second) = tokio::join!(a.execute(ra), b.execute(rb));
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    assert!(
        matches!(&first, Err(Error::Rejected(message)) if message == "Insufficient quantity or nonpositive line")
            || matches!(&second, Err(Error::Rejected(message)) if message == "Insufficient quantity or nonpositive line")
    );
    assert_eq!(
        a.snapshot()
            .await
            .unwrap()
            .quantities
            .get("p")
            .map(|q| q.quantity),
        Some(0)
    );
    assert!(matches!(
        a.execute(request(
            "empty",
            Command::CreateOrder {
                id: "third".into(),
                lines: match order() {
                    Command::CreateOrder { lines, .. } => lines,
                    _ => vec![],
                }
            }
        ))
        .await,
        Err(Error::Rejected(_))
    ));
}
#[tokio::test]
async fn failed_batch_rolls_back_journal_projection_and_identity() {
    let dir = tempfile::tempdir().unwrap();
    let b = Backend::open(dir.path().join("test.sqlite")).await.unwrap();
    {
        let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
        sqlx::raw_sql("CREATE TRIGGER reject_quantity BEFORE UPDATE ON products BEGIN SELECT RAISE(ABORT,'injected failure'); END;").execute(&mut *c).await.unwrap();
    }
    let req = request("create", create());
    assert!(matches!(
        b.execute(req.clone()).await,
        Err(Error::Uncertain(_))
    ));
    assert_eq!(b.snapshot().await.unwrap(), Inventory::default());
    {
        let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
        let count = sqlx::query_scalar!("SELECT count(*) FROM inventory_commands")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        assert_eq!(count, 0);
        sqlx::raw_sql("DROP TRIGGER reject_quantity")
            .execute(&mut *c)
            .await
            .unwrap();
    }
    assert_eq!(b.execute(req).await.unwrap().order, 2);
    let before = b.snapshot().await.unwrap();
    {
        let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
        sqlx::raw_sql("CREATE TRIGGER reject_order BEFORE INSERT ON orders BEGIN SELECT RAISE(ABORT,'injected failure'); END;").execute(&mut *c).await.unwrap();
    }
    let order_request = request("order", order());
    assert!(matches!(
        b.execute(order_request.clone()).await,
        Err(Error::Uncertain(_))
    ));
    assert_eq!(b.snapshot().await.unwrap(), before);
    {
        let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
        let quantity = sqlx::query_scalar!("SELECT quantity FROM products WHERE id='p'")
            .fetch_one(&mut *c)
            .await
            .unwrap();
        assert_eq!(quantity, 3);
        sqlx::raw_sql("DROP TRIGGER reject_order")
            .execute(&mut *c)
            .await
            .unwrap();
    }
    assert_eq!(b.execute(order_request).await.unwrap().order, 3);
}
#[tokio::test]
async fn gaps_corrupt_and_unknown_payloads_fail_closed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.sqlite");
    let b = Backend::open(&path).await.unwrap();
    b.execute(request("create", create())).await.unwrap();
    {
        let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
        sqlx::query!("UPDATE inventory_events SET \"order\"=3 WHERE \"order\"=2")
            .execute(&mut *c)
            .await
            .unwrap();
    }
    assert!(b.snapshot().await.is_err());
    assert!(b.rebuild_projections().await.is_err());
    assert!(Backend::open(&path).await.is_err());
    {
        let mut state = b.state.lock().await;
        let corrupt: &[u8] = b"not JSONB";
        sqlx::query!(
            "UPDATE inventory_events SET \"order\"=2, payload=? WHERE \"order\"=3",
            corrupt
        )
        .execute(&mut state.connection)
        .await
        .unwrap();
    }
    assert!(b.rebuild_projections().await.is_err());
    assert!(Backend::open(&path).await.is_err());
    for payload in ["{}", r#"{"UnknownEvent":{}}"#] {
        {
            let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
            sqlx::query!(
                "UPDATE inventory_events SET \"order\"=2, payload=jsonb(?) WHERE \"order\" IN (2,3)",
                payload
            )
            .execute(&mut *c)
            .await
            .unwrap();
        }
        assert!(b.rebuild_projections().await.is_err());
        assert!(Backend::open(&path).await.is_err());
    }
}
#[tokio::test]
async fn invalid_recording_metadata_fails_closed() {
    for invalid_operation in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("metadata.sqlite");
        let b = Backend::open(&path).await.unwrap();
        b.execute(request("create", create())).await.unwrap();
        b.execute(request("order", order())).await.unwrap();
        {
            let mut state = b.state.lock().await;
            if invalid_operation {
                let mut tx = state
                    .connection
                    .begin_with("BEGIN IMMEDIATE")
                    .await
                    .unwrap();
                sqlx::query!("UPDATE inventory_commands SET id=' ' WHERE id='order'")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                sqlx::query!("UPDATE inventory_events SET operation_id=' ' WHERE \"order\"=3")
                    .execute(&mut *tx)
                    .await
                    .unwrap();
                tx.commit().await.unwrap();
            } else {
                sqlx::query!("UPDATE inventory_events SET occurred_at=0 WHERE \"order\"=3")
                    .execute(&mut state.connection)
                    .await
                    .unwrap();
            }
        }
        assert!(b.rebuild_projections().await.is_err());
        assert!(Backend::open(&path).await.is_err());
    }
}

#[test]
fn malformed_event_does_not_mutate() {
    let mut i = Inventory::default();
    let bad = Event::ProductQuantityAdjusted {
        product_id: "missing".into(),
        delta: 1,
        reason: None,
    };
    assert!(i.apply(&bad).is_err());
    assert_eq!(i, Inventory::default());
}

#[test]
fn historical_order_replay_does_not_revalidate_current_catalog_quotes() {
    let mut inventory = Inventory::default();
    for event in inventory.decide(&create(), 0).unwrap() {
        inventory.apply(&event).unwrap();
    }
    let events = inventory.decide(&order(), 1234).unwrap();
    // Model a future catalog edit without introducing that feature here.
    let current = inventory.products.get_mut("p").unwrap();
    current.price = 999;
    current.name = "Renamed tea".into();
    assert!(inventory.decide(&order(), 0).is_err());
    let mut replay = inventory.clone();
    for event in &events {
        inventory.apply(event).unwrap();
        replay.apply(event).unwrap();
    }
    assert_eq!(inventory, replay);
    let historical = inventory.orders.get("order").unwrap();
    assert_eq!(historical.created_at, 1234);
    assert_eq!(historical.lines.first().unwrap().unit_price, 100);
    assert_eq!(historical.lines.first().unwrap().name, "Tea");
    let before = inventory.clone();
    let mut invalid = historical.clone();
    invalid.id = "bad".into();
    invalid.lines.first_mut().unwrap().quantity = 0;
    assert!(inventory.apply(&Event::OrderCreated(invalid)).is_err());
    assert_eq!(inventory, before);
}

#[tokio::test]
async fn cached_commands_and_snapshot_reconcile_external_commits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.sqlite");
    let a = Backend::open(&path).await.unwrap();
    let b = Backend::open(&path).await.unwrap();
    a.execute(request("create", create())).await.unwrap();
    assert_eq!(a.state.lock().await.reloads, 0);
    assert_eq!(a.snapshot().await.unwrap().order, 2);
    assert_eq!(a.state.lock().await.reloads, 0);
    b.execute(request("order", order())).await.unwrap();
    assert_eq!(b.state.lock().await.reloads, 1);
    assert_eq!(a.state.lock().await.inventory.order, 2);
    assert_eq!(a.snapshot().await.unwrap().order, 3);
    assert_eq!(a.state.lock().await.reloads, 1);
    assert_eq!(a.snapshot().await.unwrap(), b.snapshot().await.unwrap());
    assert_eq!(a.state.lock().await.reloads, 1);
    let receipt = b
        .execute(request(
            "adjust",
            Command::AdjustProductQuantity {
                product_id: "p".into(),
                delta: 1,
                reason: None,
            },
        ))
        .await
        .unwrap();
    assert_eq!(b.state.lock().await.reloads, 1);
    assert_eq!(a.state.lock().await.inventory.order, 3);
    let mut command = order();
    if let Command::CreateOrder { id, .. } = &mut command {
        *id = "second".into();
    }
    let applied = a.execute(request("stale", command)).await.unwrap();
    assert_eq!(applied.order, receipt.order + 1);
    assert_eq!(a.state.lock().await.reloads, 2);
    assert_eq!(a.state.lock().await.inventory.order, applied.order);
    assert_eq!(a.snapshot().await.unwrap(), b.snapshot().await.unwrap());
    assert_eq!(a.snapshot().await.unwrap().quantities["p"].quantity, 2);
}

#[tokio::test]
async fn failed_commit_does_not_publish_staged_cache_or_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("commit.sqlite");
    let b = Backend::open(&path).await.unwrap();
    b.execute(request("create", create())).await.unwrap();
    let before = b.state.lock().await.inventory.clone();
    {
        let mut state = b.state.lock().await;
        // Violate the deferred linkage FK only at COMMIT, after all writes succeeded.
        sqlx::raw_sql("CREATE TRIGGER fail_commit AFTER INSERT ON inventory_commands BEGIN UPDATE inventory_events SET operation_id='missing' WHERE operation_id=NEW.id; END;").execute(&mut state.connection).await.unwrap();
    }
    let req = request("order", order());
    assert!(matches!(
        b.execute(req.clone()).await,
        Err(Error::Uncertain(_))
    ));
    assert_eq!(b.state.lock().await.inventory, before);
    assert_eq!(b.snapshot().await.unwrap(), before);
    {
        let mut state = b.state.lock().await;
        sqlx::raw_sql("DROP TRIGGER fail_commit")
            .execute(&mut state.connection)
            .await
            .unwrap();
    }
    let receipt = b.execute(req.clone()).await.unwrap();
    assert_eq!(receipt.envelopes.len(), 1);
    let envelope = receipt.envelopes.first().unwrap();
    assert_eq!(envelope.operation_id, "order");
    assert!(envelope.occurred_at > 0);
    assert_eq!(envelope.order, 3);
    assert_eq!(
        b.snapshot()
            .await
            .unwrap()
            .orders
            .get("order")
            .unwrap()
            .created_at,
        envelope.occurred_at
    );
    drop(b);
    let reopened = Backend::open(&path).await.unwrap();
    assert_eq!(reopened.execute(req).await.unwrap(), receipt);
}

#[tokio::test]
async fn company_details_validate_and_persist_independently() {
    use super::CompanyDetails;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.sqlite");
    let b = Backend::open(&path).await.unwrap();
    assert_eq!(
        b.load_company_details().await.unwrap(),
        CompanyDetails::default()
    );
    let details = CompanyDetails {
        business_name: "  Test shop  ".into(),
        tax_identifier: "TEST-ID".into(),
        address: "Test street".into(),
        contact: "test@example.invalid".into(),
    };
    let saved = b.save_company_details(&details).await.unwrap();
    assert_eq!(saved.business_name, "Test shop");
    b.set_dark_theme(true).await.unwrap();
    for index in 0..4 {
        let mut invalid = saved.clone();
        match index {
            0 => invalid.business_name = "x".repeat(201),
            1 => invalid.tax_identifier = "x".repeat(65),
            2 => invalid.address = "x".repeat(301),
            _ => invalid.contact = "x".repeat(201),
        }
        assert!(matches!(
            b.save_company_details(&invalid).await,
            Err(Error::Rejected(_))
        ));
    }
    let mut invalid = saved.clone();
    invalid.contact = "bad\nline".into();
    assert!(matches!(
        b.save_company_details(&invalid).await,
        Err(Error::Rejected(_))
    ));
    assert_eq!(b.load_company_details().await.unwrap(), saved);
    {
        let mut state = b.state.lock().await;
        sqlx::raw_sql("CREATE TRIGGER fail_settings BEFORE UPDATE ON app_settings BEGIN SELECT RAISE(ABORT,'failure'); END;").execute(&mut state.connection).await.unwrap();
    }
    assert!(
        b.save_company_details(&CompanyDetails::default())
            .await
            .is_err()
    );
    assert_eq!(b.load_company_details().await.unwrap(), saved);
    assert_eq!(b.snapshot().await.unwrap(), Inventory::default());
    drop(b);
    let reopened = Backend::open(&path).await.unwrap();
    assert_eq!(reopened.load_company_details().await.unwrap(), saved);
    assert!(reopened.dark_theme().await.unwrap());
}

#[test]
#[ignore = "Invoked only by the crash recovery parent with an isolated database"]
#[expect(
    clippy::branches_sharing_code,
    reason = "The uncommitted transaction must stay alive until the parent kills this process."
)]
fn crash_child() {
    let path = std::env::var_os("FACTUROX_TEST_DB").unwrap();
    let marker = std::env::var_os("FACTUROX_TEST_MARKER").unwrap();
    let committed = std::env::var_os("FACTUROX_TEST_COMMIT").is_some();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let b = Backend::open(std::path::PathBuf::from(path)).await.unwrap();
        if committed {
            b.execute(request("create",create())).await.unwrap();
            std::fs::write(marker, b"ready").unwrap();
            std::future::pending::<()>().await;
        } else {
            let mut c = tokio::sync::MutexGuard::map(b.state.lock().await, |s| &mut s.connection);
            let mut tx = c.begin_with("BEGIN IMMEDIATE").await.unwrap();
            let payload = serde_json::to_string(&Event::ProductCreated(product())).unwrap();
            sqlx::query!("INSERT INTO inventory_events(\"order\",payload,occurred_at,operation_id) VALUES(1,jsonb(?),0,'uncommitted')",payload).execute(&mut *tx).await.unwrap();
            std::fs::write(marker, b"ready").unwrap();
            std::future::pending::<()>().await;
        }
    });
}

#[test]
fn abrupt_process_termination_preserves_only_committed_batches() {
    for committed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("crash.sqlite");
        let marker = dir.path().join("ready");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args(["--ignored", "--exact", "backend::tests::crash_child"])
            .env("FACTUROX_TEST_DB", &path)
            .env("FACTUROX_TEST_MARKER", &marker);
        if committed {
            command.env("FACTUROX_TEST_COMMIT", "1");
        }
        let mut child = command.spawn().unwrap();
        let deadline = std::time::Instant::now()
            .checked_add(std::time::Duration::from_secs(10))
            .unwrap();
        while !marker.exists() && std::time::Instant::now() < deadline {
            if child.try_wait().unwrap().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let ready = marker.exists();
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
        }
        child.wait().unwrap();
        assert!(ready, "child failed to reach crash checkpoint");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let b = Backend::open(&path).await.unwrap();
            let snapshot = b.snapshot().await.unwrap();
            assert_eq!(snapshot.order, if committed { 2 } else { 0 });
            assert_eq!(snapshot.products.len(), usize::from(committed));
            if committed {
                assert_eq!(
                    b.execute(request("create", create())).await.unwrap().order,
                    2
                );
            }
        });
    }
}

#[test]
fn computed_totals_and_current_serialization() {
    let mut order = Order {
        id: "o".into(),
        lines: vec![OrderLine {
            product_id: "p".into(),
            name: "Tea".into(),
            unit_price: 100,
            quantity: 2,
        }],
        created_at: 0,
    };
    assert_eq!(order.total().unwrap(), 200);
    order.lines.first_mut().unwrap().quantity = 3;
    order.lines.first_mut().unwrap().unit_price = 200;
    assert_eq!(order.total().unwrap(), 600);
    let json = serde_json::to_value(&order).unwrap();
    assert!(json.get("total").is_none());
    assert_eq!(serde_json::from_value::<Order>(json).unwrap(), order);
    let event = Event::OrderCreated(order.clone());
    let event_json = serde_json::to_value(&event).unwrap();
    assert!(
        event_json
            .get("OrderCreated")
            .unwrap()
            .get("total")
            .is_none()
    );
    assert_eq!(serde_json::from_value::<Event>(event_json).unwrap(), event);
    order.lines.first_mut().unwrap().unit_price = i64::MAX;
    assert!(order.total().is_err());
    order.lines.first_mut().unwrap().quantity = 1;
    let line = order.lines.first().unwrap().clone();
    order.lines.push(line);
    assert!(order.total().is_err());
}
