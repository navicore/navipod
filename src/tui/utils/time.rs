use time::OffsetDateTime;
use x509_parser::time::ASN1Time;

pub fn asn1time_to_future_days_string(asn1_time: &ASN1Time) -> String {
    let now = OffsetDateTime::now_utc();

    let target_time = asn1_time.to_datetime();

    let duration = target_time - now;
    let days_difference = duration.whole_days();

    format!("{days_difference}d")
}
