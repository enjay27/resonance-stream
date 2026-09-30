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

/// What a field holds, by wire type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    /// Wire type 0. A varint cut short reads as 0 (and consumes nothing).
    Varint(u64),
    /// Wire type 2: the body, without its length prefix, clamped to the buffer.
    Bytes(&'a [u8]),
    /// Fixed-width and group wire types (1, 5, 3, 4, 6, 7): not interpreted.
    Other,
}

/// One field of a protobuf message.
#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    /// The raw tag (`field number << 3 | wire type`); multi-byte for fields 16+.
    pub tag: u64,
    pub value: Value<'a>,
    /// Everything [`skip_field`] covers after the tag (for wire type 2 that
    /// includes the length prefix): what the unknown-field capture stores.
    pub raw: &'a [u8],
}

impl Field<'_> {
    pub fn number(&self) -> u64 {
        self.tag >> 3
    }
}

/// Walks the fields of a message. Corrupt input never panics: lengths are
/// clamped to the buffer, and the walk ends at a tag it cannot read.
#[derive(Debug, Clone)]
pub struct Fields<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Fields<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = Field<'a>;

    fn next(&mut self) -> Option<Field<'a>> {
        if self.pos >= self.data.len() {
            return None;
        }
        let (tag, tag_len) = read_tag(&self.data[self.pos..])?;
        let body = self.pos + tag_len;
        let wire_type = (tag & 0x07) as u8;
        let end = body
            .saturating_add(skip_field(wire_type, &self.data[body..]))
            .min(self.data.len());
        let value = match wire_type {
            0 => Value::Varint(read_varint(&self.data[body..]).0),
            2 => {
                let (_, prefix) = read_varint(&self.data[body..]);
                Value::Bytes(&self.data[body + prefix..end])
            }
            _ => Value::Other,
        };
        self.pos = end;
        Some(Field {
            tag,
            value,
            raw: &self.data[body..end],
        })
    }
}

/// Scans a byte slice to find and extract a specific string tag.
pub fn find_string_by_tag(data: &[u8], target_tag: u8) -> Option<String> {
    Fields::new(data).find_map(|field| match field.value {
        Value::Bytes(text) if field.tag == u64::from(target_tag) && !text.is_empty() => {
            Some(String::from_utf8_lossy(text).into_owned())
        }
        _ => None,
    })
}

/// Scans a byte slice to find and extract a specific integer tag.
pub fn find_int_by_tag(data: &[u8], target_tag: u8) -> Option<u64> {
    Fields::new(data).find_map(|field| match field.value {
        Value::Varint(value) if field.tag == u64::from(target_tag) => Some(value),
        _ => None,
    })
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

    #[test]
    fn fields_walks_each_wire_type() {
        let data = [
            0x08, 0x96, 0x01, // field 1 varint 150
            0x12, 0x02, b'h', b'i', // field 2 bytes "hi"
            0x19, 1, 2, 3, 4, 5, 6, 7, 8, // field 3 fixed64
            0x25, 9, 9, 9, 9,    // field 4 fixed32
            0x2B, // field 5 wire 3 (group start): one byte
        ];
        let fields: Vec<_> = Fields::new(&data).collect();
        let tags: Vec<_> = fields.iter().map(|f| f.tag).collect();
        assert_eq!(tags, [0x08, 0x12, 0x19, 0x25, 0x2B]);
        assert!(matches!(fields[0].value, Value::Varint(150)));
        assert!(matches!(fields[1].value, Value::Bytes(b"hi")));
        assert!(matches!(fields[2].value, Value::Other));
        assert_eq!(fields[2].raw, &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(fields[3].raw, &[9, 9, 9, 9]);
        assert_eq!((fields[0].number(), fields[1].number()), (1, 2));
    }

    #[test]
    fn fields_raw_keeps_the_length_prefix_like_skip_field() {
        // The unknown-field capture stores exactly what skip_field covered.
        let data = [0x12, 0x03, b'a', b'b', b'c'];
        let field = Fields::new(&data).next().unwrap();
        assert_eq!(field.raw, &[0x03, b'a', b'b', b'c']);
        assert!(matches!(field.value, Value::Bytes(b"abc")));
    }

    #[test]
    fn fields_clamps_an_overlong_length_and_never_overflows() {
        let data = [0x12, 0x32, 0xFF, 0xFF]; // claims 50 bytes, 2 present
        let field = Fields::new(&data).next().unwrap();
        assert!(matches!(field.value, Value::Bytes([0xFF, 0xFF])));

        let mut huge = vec![0x12];
        huge.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
        assert_eq!(Fields::new(&huge).count(), 1);
    }

    #[test]
    fn fields_keeps_the_old_loops_edge_behaviour() {
        // A varint cut short reads as 0 and consumes nothing; the walk still
        // ends. A tag that is itself cut short ends the walk.
        let data = [0x08, 0x80];
        let got: Vec<_> = Fields::new(&data).collect();
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0].value, Value::Varint(0)));
        assert_eq!(Fields::new(&[0xA0]).count(), 0);
        assert_eq!(Fields::new(&[]).count(), 0);
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
