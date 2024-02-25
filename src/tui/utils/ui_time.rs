use time::OffsetDateTime;
use x509_parser::time::ASN1Time; // Import time::Duration as TimeDuration to avoid name clash

pub fn asn1time_to_future_days_string(asn1_time: &ASN1Time) -> String {
    let now = OffsetDateTime::now_utc();

    // Directly get OffsetDateTime from ASN1Time
    let target_time = asn1_time.to_datetime();

    // Calculate the difference in days
    let duration = target_time - now;
    let days_difference = duration.whole_days();

    // Return the difference in days as a String with a "d" suffix
    format!("{days_difference}d")
}
