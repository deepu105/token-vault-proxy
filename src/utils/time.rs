/// Current time in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn now_ms_returns_reasonable_value() {
        let ts = now_ms();
        // Must be after 2024-01-01 and within a plausible range
        assert!(ts > 1_704_067_200_000, "timestamp should be after 2024");
        assert!(ts < 4_102_444_800_000, "timestamp should be before 2100");
    }
}
