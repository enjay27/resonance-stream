/// Safely reads a Protobuf Varint from a byte slice.
/// Returns a tuple of (Value, Bytes Read).
/// Returns (0, 0) if the varint is incomplete (waiting for more TCP data).
pub fn read_varint(data: &[u8]) -> (u64, usize) {
    let mut value = 0u64;
    let mut shift = 0;
    let mut pos = 0;

    while pos < data.len() {
        let byte = data[pos];
        value |= ((byte & 0x7F) as u64) << shift;
        pos += 1;

        if (byte & 0x80) == 0 {
            return (value, pos);
        }

        shift += 7;
        if shift >= 64 {
            break;
        } // Prevent panic on corrupted data
    }

    // If we exit the loop but the last byte had the continuation bit set,
    // we don't have the full Varint yet.
    (0, 0)
}

/// End of a length-delimited field that starts at `start` and claims `len`
/// bytes, clamped to `limit`. `len` comes off the wire, so the sum must not
/// overflow: a corrupted length would otherwise panic (debug) or wrap (release).
pub fn field_end(start: usize, len: u64, limit: usize) -> usize {
    usize::try_from(len)
        .ok()
        .and_then(|len| start.checked_add(len))
        .map_or(limit, |end| end.min(limit))
}

/// Reads a field tag. Tags are varints: field numbers of 16 and up take two
/// or more bytes. Returns (tag, bytes read), or `None` if incomplete.
pub fn read_tag(data: &[u8]) -> Option<(u64, usize)> {
    match read_varint(data) {
        (_, 0) => None,
        (tag, read) => Some((tag, read)),
    }
}

/// Calculates how many bytes to skip based on the Protobuf wire type.
/// Saturates instead of overflowing; callers clamp the result to their buffer.
pub fn skip_field(wire_type: u8, data: &[u8]) -> usize {
    match wire_type {
        0 => read_varint(data).1,
        1 => 8,
        2 => {
            let (len, read) = read_varint(data);
            read.saturating_add(usize::try_from(len).unwrap_or(usize::MAX))
        }
        5 => 4,
        _ => 1,
    }
}

/// Scans a byte slice to find and extract a specific string tag.
pub fn find_string_by_tag(data: &[u8], target_tag: u8) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let (tag, tag_len) = read_tag(&data[i..])?;
        let body = i + tag_len;
        if tag == u64::from(target_tag) {
            let (len, read) = read_varint(&data[body..]);
            let start = body + read;
            let end = field_end(start, len, data.len());
            if start < end {
                return Some(String::from_utf8_lossy(&data[start..end]).into_owned());
            }
        }
        i = body.saturating_add(skip_field((tag & 0x07) as u8, &data[body..]));
    }
    None
}

/// Scans a byte slice to find and extract a specific integer tag.
pub fn find_int_by_tag(data: &[u8], target_tag: u8) -> Option<u64> {
    let mut i = 0;
    while i < data.len() {
        let (tag, tag_len) = read_tag(&data[i..])?;
        let body = i + tag_len;
        if tag == u64::from(target_tag) {
            let (val, _) = read_varint(&data[body..]);
            return Some(val);
        }
        i = body.saturating_add(skip_field((tag & 0x07) as u8, &data[body..]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_varint() {
        // Test valid 1-byte varint (Value: 5)
        let data1 = [0x05];
        let (val1, len1) = read_varint(&data1);
        assert_eq!(val1, 5);
        assert_eq!(len1, 1);

        // Test valid 2-byte varint (Value: 150 -> 0x96 0x01)
        let data2 = [0x96, 0x01];
        let (val2, len2) = read_varint(&data2);
        assert_eq!(val2, 150);
        assert_eq!(len2, 2);

        // Test INCOMPLETE varint (Missing the second byte)
        let data3 = [0x96];
        let (val3, len3) = read_varint(&data3);
        assert_eq!(val3, 0);
        assert_eq!(len3, 0); // Should return 0 length indicating "need more data"
    }

    const HUGE_VARINT: [u8; 10] = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01];

    #[test]
    fn field_end_never_overflows() {
        assert_eq!(field_end(5, u64::MAX, 10), 10);
        assert_eq!(field_end(usize::MAX, 1, 10), 10);
        assert_eq!(field_end(2, 3, 10), 5);
        assert_eq!(field_end(2, 30, 10), 10);
    }

    #[test]
    fn skip_field_saturates_on_huge_length() {
        // Wire type 2 with a length of u64::MAX must not overflow.
        assert!(skip_field(2, &HUGE_VARINT) >= HUGE_VARINT.len());
    }

    #[test]
    fn find_by_tag_survives_huge_length() {
        let mut data = vec![0x12];
        data.extend_from_slice(&HUGE_VARINT);
        assert_eq!(find_string_by_tag(&data, 0x0A), None);
        assert_eq!(find_int_by_tag(&data, 0x10), None);
    }

    #[test]
    fn read_tag_decodes_multi_byte_tags() {
        // Field 20, wire type 0 -> tag 160 -> varint [0xA0, 0x01]
        assert_eq!(read_tag(&[0xA0, 0x01, 0x05]), Some((160, 2)));
        assert_eq!(read_tag(&[0x08, 0x01]), Some((8, 1)));
        assert_eq!(read_tag(&[0xA0]), None); // incomplete
        assert_eq!(read_tag(&[]), None);
    }
}
