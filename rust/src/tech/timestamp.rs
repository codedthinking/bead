use chrono::{DateTime, Utc, TimeZone, FixedOffset, Timelike};
use crate::error::{BeadError, Result};

/// Generate a timestamp in bead format: YYYYMMDDTHHMMSSNNNNNN±ZZZZ
pub fn timestamp() -> String {
    let now = Utc::now();
    format!("{}", now.format("%Y%m%dT%H%M%S%6f%z"))
}

/// Parse a bead timestamp back to DateTime
pub fn parse_timestamp(timestamp: &str) -> Result<DateTime<Utc>> {
    // Handle the bead timestamp format: YYYYMMDDTHHMMSSNNNNNN±ZZZZ
    // Example: 20240115T143022123456+0100
    
    if timestamp.len() < 24 {
        return Err(BeadError::InvalidInput(format!("Invalid timestamp: {}", timestamp)));
    }

    // Parse components
    let year = timestamp[0..4].parse::<i32>()
        .map_err(|_| BeadError::InvalidInput("Invalid year in timestamp".into()))?;
    let month = timestamp[4..6].parse::<u32>()
        .map_err(|_| BeadError::InvalidInput("Invalid month in timestamp".into()))?;
    let day = timestamp[6..8].parse::<u32>()
        .map_err(|_| BeadError::InvalidInput("Invalid day in timestamp".into()))?;
    
    let hour = timestamp[9..11].parse::<u32>()
        .map_err(|_| BeadError::InvalidInput("Invalid hour in timestamp".into()))?;
    let minute = timestamp[11..13].parse::<u32>()
        .map_err(|_| BeadError::InvalidInput("Invalid minute in timestamp".into()))?;
    let second = timestamp[13..15].parse::<u32>()
        .map_err(|_| BeadError::InvalidInput("Invalid second in timestamp".into()))?;
    
    let microsecond = if timestamp.len() >= 21 {
        timestamp[15..21].parse::<u32>()
            .map_err(|_| BeadError::InvalidInput("Invalid microsecond in timestamp".into()))?
    } else {
        0
    };

    // Parse timezone offset
    let tz_offset = if timestamp.len() >= 24 {
        let tz_str = &timestamp[21..];
        let sign = if tz_str.starts_with('+') { 1 } else { -1 };
        let hours = tz_str[1..3].parse::<i32>()
            .unwrap_or(0);
        let minutes = if tz_str.len() >= 5 {
            tz_str[3..5].parse::<i32>().unwrap_or(0)
        } else {
            0
        };
        FixedOffset::east_opt(sign * (hours * 3600 + minutes * 60))
            .ok_or_else(|| BeadError::InvalidInput("Invalid timezone offset".into()))?
    } else {
        FixedOffset::east_opt(0).unwrap()
    };

    // Create DateTime
    let dt = tz_offset
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .ok_or_else(|| BeadError::InvalidInput("Invalid datetime components".into()))?
        .with_nanosecond(microsecond * 1000)
        .ok_or_else(|| BeadError::InvalidInput("Invalid microseconds".into()))?;

    Ok(dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_timestamp_generation() {
        let ts = timestamp();
        assert!(ts.len() >= 24);
        assert!(ts.contains('T'));
        assert!(ts.chars().nth(8) == Some('T'));
    }

    #[test]
    fn test_parse_timestamp() {
        let timestamp_str = "20240115T143022123456+0100";
        let result = parse_timestamp(timestamp_str);
        assert!(result.is_ok());
        
        let dt = result.unwrap();
        assert_eq!(dt.year(), 2024);
        assert_eq!(dt.month(), 1);
        assert_eq!(dt.day(), 15);
        assert_eq!(dt.hour(), 13); // Adjusted for UTC from +0100
        assert_eq!(dt.minute(), 30);
        assert_eq!(dt.second(), 22);
    }

    #[test]
    fn test_parse_timestamp_negative_offset() {
        let timestamp_str = "20240115T143022123456-0500";
        let result = parse_timestamp(timestamp_str);
        assert!(result.is_ok());
        
        let dt = result.unwrap();
        assert_eq!(dt.hour(), 19); // Adjusted for UTC from -0500
    }

    #[test]
    fn test_parse_invalid_timestamp() {
        assert!(parse_timestamp("invalid").is_err());
        assert!(parse_timestamp("2024").is_err());
        assert!(parse_timestamp("20240115").is_err());
    }
}