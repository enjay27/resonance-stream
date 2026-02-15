// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    resonance_stream_lib::run()
}

#[cfg(test)]
mod tests {
    use etherparse::{PacketBuilder, PacketHeaders, TransportHeader, NetHeaders};

    #[test]
    fn test_extract_tcp_chat_payload() {
        // 1. SETUP: Build a fake BPSR packet
        // (PacketBuilder syntax remains the same)
        let builder = PacketBuilder::
        ethernet2([1, 2, 3, 4, 5, 6], [6, 5, 4, 3, 2, 1])
            .ipv4([192, 168, 1, 1], [192, 168, 1, 2], 64)
            .tcp(51000, 12345, 1, 0)
            .fin();

        let chat_message = "Hello BPSR User!";
        let mut packet_bytes = Vec::<u8>::new();
        builder.write(&mut packet_bytes, chat_message.as_bytes()).unwrap();

        // ---------------------------------------------------------
        // 2. THE LOGIC (Updated for 0.19)
        // ---------------------------------------------------------

        // Use 'PacketHeaders' to parse everything at once (Lazy/Slicing style)
        // This is the preferred way in 0.19 to get the payload easily.
        let parsed = PacketHeaders::from_ethernet_slice(&packet_bytes).expect("Failed to parse packet");

        // VALIDATION: Check for BPSR Port (51000)
        // 'parsed.transport' contains the TransportHeader enum (Tcp/Udp)
        if let Some(TransportHeader::Tcp(tcp_header)) = parsed.transport {

            // Check destination port (or source port)
            assert_eq!(tcp_header.source_port, 51000);

            // 3. PAYLOAD EXTRACTION
            let payload_wrapper = parsed.payload;
            let payload_bytes = payload_wrapper.slice();
            let extracted_text = String::from_utf8_lossy(payload_bytes);

            println!("Extracted: {}", extracted_text);
            assert_eq!(extracted_text, "Hello BPSR User!");

        } else {
            panic!("Packet was not TCP!");
        }

        // OPTIONAL: Accessing IP headers in 0.19 (Renamed to 'net')
        if let Some(NetHeaders::Ipv4(ipv4_header, _extensions)) = parsed.net {
            println!("Source IP: {:?}", ipv4_header.source);
        }
    }
}