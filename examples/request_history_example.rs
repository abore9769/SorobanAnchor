use anchorkit::request_record::{RequestRecord, RequestRecordStore, RequestRetentionPolicy};

fn main() {
    let policy = RequestRetentionPolicy::new(100, 0, true);
    let mut history = RequestRecordStore::new(policy);

    history.push(
        RequestRecord::new(
            1001,
            1_700_000_000,
            "deposit",
            "synthetic-actor-1",
            "accepted",
            Some("synthetic request"),
        ),
        0,
    );
    history.push(
        RequestRecord::new(
            1002,
            1_700_000_060,
            "withdrawal",
            "synthetic-actor-2",
            "rejected",
            None::<&str>,
        ),
        0,
    );

    let page = history.export_batch(0, 1);
    println!("First page: {} record(s), more: {}", page.records.len(), page.has_more);

    if let Some(record) = history.get_by_id(1002) {
        println!("Found request {}: {} ({})", record.id, record.operation, record.outcome);
    }
}
