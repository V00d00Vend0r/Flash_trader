use ravenslinger_data::PriceDataBuffer;
use chrono::{Utc, Duration};

#[test]
fn integration_import_and_basic_ops() {
    let mut b = PriceDataBuffer::new(2);
    let t = Utc::now();
    b.push_raw(t, 1.0, None);
    b.push_raw(t + Duration::seconds(1), 2.0, None);
    b.push_raw(t + Duration::seconds(2), 3.0, None);

    assert_eq!(b.len(), 2);
    assert_eq!(b.last().unwrap().price, 3.0);
}
