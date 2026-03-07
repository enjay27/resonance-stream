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
        if shift >= 64 { break; } // Prevent panic on corrupted data
    }

    // If we exit the loop but the last byte had the continuation bit set,
    // we don't have the full Varint yet.
    (0, 0)
}

/// Calculates how many bytes to skip based on the Protobuf wire type.
pub fn skip_field(wire_type: u8, data: &[u8]) -> usize {
    match wire_type {
        0 => read_varint(data).1,
        1 => 8,
        2 => {
            let (len, read) = read_varint(data);
            read + len as usize
        }
        5 => 4,
        _ => 1,
    }
}

/// Scans a byte slice to find and extract a specific string tag.
pub fn find_string_by_tag(data: &[u8], target_tag: u8) -> Option<String> {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        if tag == target_tag {
            let (len, read) = read_varint(&data[i+1..]);
            let start = i + 1 + read;
            let end = (start + len as usize).min(data.len());
            if start < end {
                return Some(String::from_utf8_lossy(&data[start..end]).into_owned());
            }
        }
        let wire_type = tag & 0x07;
        i += 1 + skip_field(wire_type, &data[i+1..]);
    }
    None
}

/// Scans a byte slice to find and extract a specific integer tag.
pub fn find_int_by_tag(data: &[u8], target_tag: u8) -> Option<u64> {
    let mut i = 0;
    while i < data.len() {
        let tag = data[i];
        if tag == target_tag {
            let (val, _) = read_varint(&data[i+1..]);
            return Some(val);
        }
        let wire_type = tag & 0x07;
        i += 1 + skip_field(wire_type, &data[i+1..]);
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
}