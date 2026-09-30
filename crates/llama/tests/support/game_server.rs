//! Mock game server: encodes chat the way port 5003 carries it (as far as
//! the parser knows the layout), frames it, cuts the stream into TCP
//! segments and wraps each in an IPv4/TCP packet, as the raw socket sees it.

use etherparse::PacketBuilder;
use std::sync::atomic::{AtomicU32, Ordering};

pub const SERVER: [u8; 4] = [43, 155, 1, 1];
pub const CLIENT: [u8; 4] = [192, 168, 0, 10];
pub const GAME_PORT: u16 = 5003;
/// Frame types (read off a real capture). Bit 15 = the body is zstd.
pub const TYPE_CHAT: u16 = 0x0002;
pub const TYPE_OWN_LINE: u16 = 0x0003;
pub const TYPE_KEEPALIVE: u16 = 0x0004;
pub const COMPRESSED: u16 = 0x8000;

// --- protobuf-style encoding ---

pub fn varint(mut v: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(byte);
            return out;
        }
        out.push(byte | 0x80);
    }
}

pub fn var_field(field: u64, v: u64) -> Vec<u8> {
    [varint(field << 3), varint(v)].concat()
}

pub fn bytes_field(field: u64, body: &[u8]) -> Vec<u8> {
    [
        varint(field << 3 | 2),
        varint(body.len() as u64),
        body.to_vec(),
    ]
    .concat()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Channel {
    World,
    Local,
    Party,
    Guild,
    Beginner,
}

impl Channel {
    fn code(self) -> u64 {
        match self {
            Channel::World => 1,
            Channel::Local => 2,
            Channel::Party => 3,
            Channel::Guild => 4,
            Channel::Beginner => 9,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Channel::World => "WORLD",
            Channel::Local => "LOCAL",
            Channel::Party => "PARTY",
            Channel::Guild => "GUILD",
            Channel::Beginner => "BEGINNER",
        }
    }
}

/// One piece of a rich message (message block field 7).
#[derive(Clone, Debug)]
pub enum Chunk {
    Text(String),
    ItemLink,
}

#[derive(Clone, Debug)]
enum Body {
    Text(String),
    Rich(Vec<Chunk>),
}

/// A chat line from another player (root field 2).
#[derive(Clone, Debug)]
pub struct Chat {
    pub channel: Channel,
    pub seq: u64,
    pub uid: u64,
    pub nickname: String,
    pub level: u64,
    pub timestamp: u64,
    body: Body,
    extra: Vec<u8>,
}

pub fn chat(seq: u64, uid: u64, nickname: &str, text: &str) -> Chat {
    Chat {
        channel: Channel::World,
        seq,
        uid,
        nickname: nickname.to_string(),
        level: 60,
        timestamp: 1_772_343_736 + seq,
        body: Body::Text(text.to_string()),
        extra: Vec::new(),
    }
}

impl Chat {
    pub fn on(mut self, channel: Channel) -> Self {
        self.channel = channel;
        self
    }

    pub fn rich(mut self, chunks: Vec<Chunk>) -> Self {
        self.body = Body::Rich(chunks);
        self
    }

    /// A chat-payload field the parser does not know (field 9, varint).
    pub fn with_unknown_field(mut self) -> Self {
        self.extra = var_field(9, 777);
        self
    }

    pub fn payload(&self) -> Vec<u8> {
        let sender = [
            var_field(1, self.uid),
            bytes_field(2, self.nickname.as_bytes()),
            var_field(5, self.level),
        ]
        .concat();
        let message = match &self.body {
            Body::Text(text) => bytes_field(3, text.as_bytes()),
            Body::Rich(chunks) => {
                let rich: Vec<u8> = chunks
                    .iter()
                    .flat_map(|chunk| {
                        let chunk = match chunk {
                            Chunk::Text(t) => [
                                var_field(1, 7),
                                bytes_field(2, &bytes_field(1, t.as_bytes())),
                            ]
                            .concat(),
                            Chunk::ItemLink => [var_field(1, 3), bytes_field(2, &[])].concat(),
                        };
                        bytes_field(2, &chunk)
                    })
                    .collect();
                bytes_field(7, &rich)
            }
        };
        [
            var_field(1, self.seq),
            bytes_field(2, &sender),
            var_field(3, self.timestamp),
            bytes_field(4, &message),
            self.extra.clone(),
        ]
        .concat()
    }

    /// The protobuf root: `0x0A len { channel, field 2: chat }`.
    pub fn root(&self) -> Vec<u8> {
        root(
            [
                var_field(1, self.channel.code()),
                bytes_field(2, &self.payload()),
            ]
            .concat(),
        )
    }

    pub fn frame(&self) -> Vec<u8> {
        frame(&self.root())
    }
}

/// A line the player sent (root field 4); it carries no sender or ids.
pub fn me_frame(channel: Channel, text: &str) -> Vec<u8> {
    let block = [
        var_field(2, channel.code()),
        bytes_field(3, text.as_bytes()),
    ]
    .concat();
    frame(&root(bytes_field(4, &block)))
}

pub fn root(fields: Vec<u8>) -> Vec<u8> {
    [vec![0x0A], varint(fields.len() as u64), fields].concat()
}

/// A live chat frame: `[len][0x0002][16-byte header][root]`.
pub fn frame(root: &[u8]) -> Vec<u8> {
    frame_of(TYPE_CHAT, 16, root)
}

/// `[u32 length, header included][u16 type][header][payload]`. The inner
/// header's contents are not known; the game's varies, ours is zeros.
pub fn frame_of(ty: u16, header_len: usize, payload: &[u8]) -> Vec<u8> {
    let len = (6 + header_len + payload.len()) as u32;
    [
        &len.to_be_bytes()[..],
        &ty.to_be_bytes(),
        &vec![0u8; header_len],
        payload,
    ]
    .concat()
}

/// A 6-byte keepalive frame; the game sends these all the time.
pub fn keepalive() -> Vec<u8> {
    frame_of(TYPE_KEEPALIVE, 0, &[])
}

/// The channel history the server re-sends (type 0x8003, zstd): `{ 3: channel,
/// 5: chat, ... }`, newest first, so `lines` (oldest first) are reversed.
pub fn history_frame(channel: Channel, lines: &[Chat]) -> Vec<u8> {
    let mut fields = var_field(3, channel.code());
    for line in lines.iter().rev() {
        fields.extend(bytes_field(5, &line.payload()));
    }
    let packed = ruzstd::encoding::compress_to_vec(
        &root(fields)[..],
        ruzstd::encoding::CompressionLevel::Fastest,
    );
    frame_of(COMPRESSED | TYPE_OWN_LINE, 12, &packed)
}

// --- cutting a byte stream into TCP segments ---

/// The stream cut at the given offsets (ascending, inside the stream).
pub fn cut_at(stream: &[u8], cuts: &[usize]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut start = 0;
    for &cut in cuts {
        out.push(stream[start..cut].to_vec());
        start = cut;
    }
    out.push(stream[start..].to_vec());
    out.retain(|s| !s.is_empty());
    out
}

/// The stream in segments of `size` bytes (the last may be shorter).
pub fn cut_every(stream: &[u8], size: usize) -> Vec<Vec<u8>> {
    stream.chunks(size).map(<[u8]>::to_vec).collect()
}

// --- the TCP connection ---

/// One TCP connection from the game server to one client. Sequence numbers
/// advance with each payload, as a real sender's do.
pub struct Connection {
    pub server: [u8; 4],
    pub server_port: u16,
    pub client: [u8; 4],
    pub client_port: u16,
    next_seq: u32,
}

impl Connection {
    /// Each connection starts at its own sequence range, as separate TCP
    /// connections do; two of them never look like a retransmit of each other.
    pub fn new(client: [u8; 4], client_port: u16) -> Self {
        static NEXT_START: AtomicU32 = AtomicU32::new(1_000);
        let start = NEXT_START.fetch_add(1 << 20, Ordering::Relaxed);
        Self {
            server: SERVER,
            server_port: GAME_PORT,
            client,
            client_port,
            next_seq: start,
        }
    }

    pub fn to_default_client() -> Self {
        Self::new(CLIENT, 50_000)
    }

    /// The sequence number the next segment carries.
    pub fn seq(&self) -> u32 {
        self.next_seq
    }

    /// The next segment of the stream as an IPv4/TCP packet.
    pub fn packet(&mut self, payload: &[u8]) -> Vec<u8> {
        let seq = self.next_seq;
        self.next_seq = seq.wrapping_add(payload.len() as u32);
        self.packet_at(seq, payload)
    }

    /// A packet with an explicit sequence number (retransmits, reordering).
    pub fn packet_at(&self, seq: u32, payload: &[u8]) -> Vec<u8> {
        let builder = PacketBuilder::ipv4(self.server, self.client, 64).tcp(
            self.server_port,
            self.client_port,
            seq,
            65_535,
        );
        let mut out = Vec::with_capacity(builder.size(payload.len()));
        builder.write(&mut out, payload).unwrap();
        out
    }

    pub fn packets(&mut self, segments: &[Vec<u8>]) -> Vec<Vec<u8>> {
        segments.iter().map(|s| self.packet(s)).collect()
    }
}

/// Same payload as an IPv6/TCP packet from port 5003 (not captured).
pub fn ipv6_packet(payload: &[u8]) -> Vec<u8> {
    let builder = PacketBuilder::ipv6([0x20; 16], [0x21; 16], 64).tcp(GAME_PORT, 50_000, 1, 65_535);
    let mut out = Vec::new();
    builder.write(&mut out, payload).unwrap();
    out
}

/// Same payload as a UDP packet from port 5003 (not captured).
pub fn udp_packet(payload: &[u8]) -> Vec<u8> {
    let builder = PacketBuilder::ipv4(SERVER, CLIENT, 64).udp(GAME_PORT, 50_000);
    let mut out = Vec::new();
    builder.write(&mut out, payload).unwrap();
    out
}
