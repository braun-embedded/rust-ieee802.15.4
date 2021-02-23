//! Partial implementation of the IEEE 802.15.4 Frame
//!
//! The main type in this module is [Frame], a type that represents an IEEE
//! 802.15.4 MAC frame. The other types in this module are supporting types
//! that are either part of [Frame] or are required to support its API.
//!
//! [Frame]: struct.Frame.html

// TODO:
// - change &mut [u8] -> bytes::BufMut
// - change &[u8] => bytes::Buf
// - remove one variant enums

use crate::mac::beacon::Beacon;
use crate::mac::command::Command;

mod frame_control;
pub mod header;
use byte::{check_len, BytesExt, TryRead, TryWrite, BE, LE};
use header::FrameType;
pub use header::Header;

/// An IEEE 802.15.4 MAC frame
///
/// Represents a MAC frame. Can be used to [decode] a frame from bytes, or
/// [encode] a frame to bytes.
///
/// [decode]: #method.decode
/// [encode]: #method.encode
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Frame<'p> {
    /// Header
    pub header: Header,

    /// Content
    pub content: FrameContent,

    /// Payload
    pub payload: &'p [u8],

    /// Footer
    ///
    /// This is a 2-byte CRC checksum.
    ///
    /// When creating an instance of this struct for encoding, you don't
    /// necessarily need to write an actual CRC checksum here. [`Frame::encode`]
    /// can omit writing this checksum, for example if the transceiver hardware
    /// automatically adds the checksum for you.
    pub footer: [u8; 2],
}

impl TryWrite for Frame<'_> {
    fn try_write(self, bytes: &mut [u8], _ctx: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        bytes.write(offset, self.header)?;
        bytes.write(offset, self.content)?;
        bytes.write(offset, self.payload)?;
        Ok(*offset)
    }
}

impl<'a> TryRead<'a> for Frame<'a> {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let header = bytes.read(offset)?;
        let content = bytes.read_with(offset, &header)?;
        Ok((
            Frame {
                header: header,
                content: content,
                payload: &bytes[*offset..],
                footer: [0; 2],
            },
            *offset,
        ))
    }
}

// impl<'p> Frame<'p> {
/// Decodes a frame from a byte buffer
///
/// # Errors
///
/// This function returns an error, if the bytes either don't encode a valid
/// IEEE 802.15.4 frame, or encode a frame that is not fully supported by
/// this implementation. Please refer to [`DecodeError`] for details.
///
/// # Example
///
/// ``` rust
/// use ieee802154::mac::frame::{
///     Frame,
///     header::{
///       Address,
///       ShortAddress,
///       FrameType,
///       PanId,
///       Security
/// }};
/// use byte::BytesExt;
///
/// # fn main() -> Result<(), ::ieee802154::mac::frame::DecodeError> {
/// // Construct a simple MAC frame. The CRC checksum (the last 2 bytes) is
/// // invalid, for the sake of convenience.
/// let bytes = [
///     0x01u8, 0x98,             // frame control
///     0x00,                   // sequence number
///     0x12, 0x34, 0x56, 0x78, // PAN identifier and address of destination
///     0x12, 0x34, 0x9a, 0xbc, // PAN identifier and address of source
///     0xde, 0xf0,             // payload
/// ];
///
/// let frame: Frame = bytes.read(&mut 0).unwrap();
/// let header = frame.header;
///
/// assert_eq!(frame.header.seq,       0x00);
/// assert_eq!(header.frame_type,      FrameType::Data);
/// assert_eq!(header.security,        Security::None);
/// assert_eq!(header.frame_pending,   false);
/// assert_eq!(header.ack_request,     false);
/// assert_eq!(header.pan_id_compress, false);
///
/// assert_eq!(
///     frame.header.destination,
///     Some(Address::Short(PanId(0x3412), ShortAddress(0x7856)))
/// );
/// assert_eq!(
///     frame.header.source,
///     Some(Address::Short(PanId(0x3412), ShortAddress(0xbc9a)))
/// );
///
/// assert_eq!(frame.payload, &[0xde, 0xf0]);
/// #
/// # Ok(())
/// # }
/// ```
/// Encodes the frame into a buffer
///
/// # Example
///
/// ## allocation allowed
/// ``` rust
/// use ieee802154::mac::{
///   Frame,
///   FrameContent,
///   WriteFooter,
///   Address,
///   ShortAddress,
///   FrameType,
///   FrameVersion,
///   Header,
///   PanId,
///   Security,
/// };
/// use byte::BytesExt;
///
/// let frame = Frame {
///     header: Header {
///         frame_type:      FrameType::Data,
///         security:        Security::None,
///         frame_pending:   false,
///         ack_request:     false,
///         pan_id_compress: false,
///         version:         FrameVersion::Ieee802154_2006,
///
///         seq:             0x00,
///         destination: Some(Address::Short(PanId(0x1234), ShortAddress(0x5678))),
///         source:      Some(Address::Short(PanId(0x1234), ShortAddress(0x9abc))),
///     },
///     content: FrameContent::Data,
///     payload: &[0xde, 0xf0],
///     footer:  [0x12, 0x34]
/// };
///
/// // Work also with `let mut bytes = Vec::new()`;
/// let mut bytes = [0u8; 32];
/// let mut len = 0usize;
///
/// bytes.write(&mut len, frame).unwrap();
///
/// let expected_bytes = [
///     0x01, 0x98,             // frame control
///     0x00,                   // sequence number
///     0x34, 0x12, 0x78, 0x56, // PAN identifier and address of destination
///     0x34, 0x12, 0xbc, 0x9a, // PAN identifier and address of source
///     0xde, 0xf0,             // payload
///    // footer, not written
/// ];
/// assert_eq!(bytes[..len], expected_bytes);
/// ```
/// ## When allocation is not an option
///
/// [`BufMut`] is implemented for `&mut [u8]` but there are common problems:
/// - panic when try put more data than capacity
/// - access to written bytes require some boilerplate
///
/// We recommend to use [`SafeBytesSlice`] as wrapper.
///
/// ``` rust
/// # use ieee802154::mac::{
/// #   Frame,
/// #   FrameContent,
/// #   WriteFooter,
/// #   Address,
/// #   ShortAddress,
/// #   FrameType,
/// #   FrameVersion,
/// #   Header,
/// #   PanId,
/// #   Security,
/// # };
/// # use byte::BytesExt;
/// #
/// # let frame = Frame {
/// #     header: Header {
/// #         frame_type:      FrameType::Data,
/// #         security:        Security::None,
/// #         frame_pending:   false,
/// #         ack_request:     false,
/// #         pan_id_compress: false,
/// #         version:         FrameVersion::Ieee802154_2006,
/// #
/// #         seq:             0x00,
/// #         destination: Some(Address::Short(PanId(0x1234), ShortAddress(0x5678))),
/// #         source:      Some(Address::Short(PanId(0x1234), ShortAddress(0x9abc))),
/// #     },
/// #     content: FrameContent::Data,
/// #     payload: &[0xde, 0xf0],
/// #     footer:  [0x12, 0x34]
/// # };
/// # let expected_bytes = [
/// #     0x01, 0x98,             // frame control
/// #     0x00,                   // sequence number
/// #     0x34, 0x12, 0x78, 0x56, // PAN identifier and address of destination
/// #     0x34, 0x12, 0xbc, 0x9a, // PAN identifier and address of source
/// #     0xde, 0xf0,             // payload
/// #    // footer, not written
/// # ];
///
/// /* Note */
/// /* variables `frame` and `expected_bytes` are the same as in example above */
///
/// /* Example use raw `&mut [u8]`  */
/// let mut bytes = [0u8; 64];
/// let mut len = 0usize;
/// // assume frame is the same as in example above
/// bytes.write(&mut len, frame);
/// assert_eq!(bytes[..len], expected_bytes);
/// ```
///
/// Tells [`Frame::encode`] whether to write the footer
///
/// Eventually, this should support three options:
/// - Don't write the footer
/// - Calculate the 2-byte CRC checksum and write that as the footer
/// - Write the footer as written into the `footer` field
///
/// For now, only not writing the footer is supported.
///
/// [`Frame::encode`](Frame::encode)
pub enum WriteFooter {
    /// Don't write the footer
    No,
}

/// Content of a frame
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FrameContent {
    /// Beacon frame content
    Beacon(Beacon),
    /// Data frame
    Data,
    /// Acknowledgement frame
    Acknowledgement,
    /// MAC command frame
    Command(Command),
}

impl TryWrite for FrameContent {
    fn try_write(self, bytes: &mut [u8], _ctx: ()) -> byte::Result<usize> {
        let offset = &mut 0;
        match self {
            FrameContent::Beacon(beacon) => bytes.write(offset, beacon),
            FrameContent::Data | FrameContent::Acknowledgement => Ok(()),
            FrameContent::Command(command) => bytes.write(offset, command),
        };
        Ok(*offset)
    }
}

impl TryRead<'_, &Header> for FrameContent {
    fn try_read(bytes: &[u8], header: &Header) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        Ok((
            match header.frame_type {
                FrameType::Beacon => FrameContent::Beacon(bytes.read(offset)?),
                FrameType::Data => FrameContent::Data,
                FrameType::Acknowledgement => FrameContent::Acknowledgement,
                FrameType::MacCommand => FrameContent::Command(bytes.read(offset)?),
            },
            *offset,
        ))
    }
}

/// Signals an error that occured while decoding bytes
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DecodeError {
    /// Buffer does not contain enough bytes
    NotEnoughBytes,

    /// The frame type is invalid
    InvalidFrameType(u8),

    /// The frame has the security bit set, which is not supported
    SecurityNotSupported,

    /// The frame's address mode is invalid
    InvalidAddressMode(u8),

    /// The frame's version is invalid or not supported
    InvalidFrameVersion(u8),

    /// The data stream contains an invalid value
    InvalidValue,
}

impl From<DecodeError> for byte::Error {
    fn from(e: DecodeError) -> Self {
        match e {
            _NotEnoughBytes => byte::Error::Incomplete,
            _InvalidFrameType => byte::Error::BadInput {
                err: "InvalidFrameType",
            },
            _SecurityNotSupported => byte::Error::BadInput {
                err: "SecurityNotSupported",
            },
            _InvalidAddressMode => byte::Error::BadInput {
                err: "InvalidAddressMode",
            },
            _InvalidFrameVersion => byte::Error::BadInput {
                err: "InvalidFrameVersion",
            },
            _InvalidValue => byte::Error::BadInput {
                err: "InvalidValue",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mac::beacon;
    use crate::mac::command;
    use crate::mac::{Address, ExtendedAddress, FrameVersion, PanId, Security, ShortAddress};

    #[test]
    fn decode_ver0_pan_id_compression() {
        let data = [
            0x41, 0x88, 0x91, 0x8f, 0x20, 0xff, 0xff, 0x33, 0x44, 0x00, 0x00,
        ];
        let frame: Frame = data.read(&mut 0).unwrap();
        let hdr = frame.header;
        assert_eq!(hdr.frame_type, FrameType::Data);
        assert_eq!(hdr.security, Security::None);
        assert_eq!(hdr.frame_pending, false);
        assert_eq!(hdr.ack_request, false);
        assert_eq!(hdr.pan_id_compress, true);
        assert_eq!(hdr.version, FrameVersion::Ieee802154_2003);
        assert_eq!(
            frame.header.destination,
            Some(Address::Short(PanId(0x208f), ShortAddress(0xffff)))
        );
        assert_eq!(
            frame.header.source,
            Some(Address::Short(PanId(0x208f), ShortAddress(0x4433)))
        );
        assert_eq!(frame.header.seq, 145);
    }

    #[test]
    fn decode_ver0_pan_id_compression_bad() {
        let data = [
            0x41, 0x80, 0x91, 0x8f, 0x20, 0xff, 0xff, 0x33, 0x44, 0x00, 0x00,
        ];
        let frame = data.read::<Frame>(&mut 0);
        assert!(frame.is_err());
        if let Err(e) = frame {
            assert_eq!(
                e,
                byte::Error::BadInput {
                    err: "InvalidAddressMode"
                }
            )
        }
    }

    #[test]
    fn decode_ver0_extended() {
        let data = [
            0x21, 0xc8, 0x8b, 0xff, 0xff, 0x02, 0x00, 0x23, 0x00, 0x60, 0xe2, 0x16, 0x21, 0x1c,
            0x4a, 0xc2, 0xae, 0xaa, 0xbb, 0xcc,
        ];
        let frame: Frame = data.read(&mut 0).unwrap();
        let hdr = frame.header;
        assert_eq!(hdr.frame_type, FrameType::Data);
        assert_eq!(hdr.security, Security::None);
        assert_eq!(hdr.frame_pending, false);
        assert_eq!(hdr.ack_request, true);
        assert_eq!(hdr.pan_id_compress, false);
        assert_eq!(hdr.version, FrameVersion::Ieee802154_2003);
        assert_eq!(
            frame.header.destination,
            Some(Address::Short(PanId(0xffff), ShortAddress(0x0002)))
        );
        assert_eq!(
            frame.header.source,
            Some(Address::Extended(
                PanId(0x0023),
                ExtendedAddress(0xaec24a1c2116e260)
            ))
        );
        assert_eq!(frame.header.seq, 139);
    }

    #[test]
    fn encode_ver0_short() {
        let frame = Frame {
            header: Header {
                frame_type: FrameType::Data,
                security: Security::None,
                frame_pending: false,
                ack_request: false,
                pan_id_compress: false,
                version: FrameVersion::Ieee802154_2003,
                destination: Some(Address::Short(PanId(0x1234), ShortAddress(0x5678))),
                source: Some(Address::Short(PanId(0x4321), ShortAddress(0x9abc))),
                seq: 0x01,
            },
            content: FrameContent::Data,
            payload: &[0xde, 0xf0],
            footer: [0x00, 0x00],
        };
        let mut buf = [0u8; 32];
        let mut len = 0usize;
        buf.write(&mut len, frame).unwrap();
        assert_eq!(len, 13);
        assert_eq!(
            buf[..len],
            [0x01, 0x88, 0x01, 0x34, 0x12, 0x78, 0x56, 0x21, 0x43, 0xbc, 0x9a, 0xde, 0xf0]
        );
    }

    #[test]
    fn encode_ver1_extended() {
        let frame = Frame {
            header: Header {
                frame_type: FrameType::Beacon,
                security: Security::None,
                frame_pending: true,
                ack_request: false,
                pan_id_compress: false,
                version: FrameVersion::Ieee802154_2006,
                destination: Some(Address::Extended(
                    PanId(0x1234),
                    ExtendedAddress(0x1122334455667788),
                )),
                source: Some(Address::Short(PanId(0x4321), ShortAddress(0x9abc))),
                seq: 0xff,
            },
            content: FrameContent::Beacon(beacon::Beacon {
                superframe_spec: beacon::SuperframeSpecification {
                    beacon_order: beacon::BeaconOrder::OnDemand,
                    superframe_order: beacon::SuperframeOrder::Inactive,
                    final_cap_slot: 15,
                    battery_life_extension: false,
                    pan_coordinator: false,
                    association_permit: false,
                },
                guaranteed_time_slot_info: beacon::GuaranteedTimeSlotInformation::new(),
                pending_address: beacon::PendingAddress::new(),
            }),
            payload: &[0xde, 0xf0],
            footer: [0x00, 0x00],
        };
        let mut buf = [0u8; 32];
        let mut len = 0usize;
        buf.write(&mut len, frame).unwrap();
        assert_eq!(len, 23);
        assert_eq!(
            buf[..len],
            [
                0x10, 0x9c, 0xff, 0x34, 0x12, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x21,
                0x43, 0xbc, 0x9a, 0xff, 0x0f, 0x00, 0x00, 0xde, 0xf0
            ]
        );
    }

    #[test]
    fn encode_ver0_pan_compress() {
        let frame = Frame {
            header: Header {
                frame_type: FrameType::Acknowledgement,
                security: Security::None,
                frame_pending: false,
                ack_request: false,
                pan_id_compress: true,
                version: FrameVersion::Ieee802154_2003,
                destination: Some(Address::Extended(
                    PanId(0x1234),
                    ExtendedAddress(0x1122334455667788),
                )),
                source: Some(Address::Short(PanId(0x1234), ShortAddress(0x9abc))),
                seq: 0xff,
            },
            content: FrameContent::Acknowledgement,
            payload: &[],
            footer: [0x00, 0x00],
        };
        let mut buf = [0u8; 32];
        let mut len = 0usize;
        buf.write(&mut len, frame).unwrap();
        assert_eq!(len, 15);
        assert_eq!(
            buf[..len],
            [
                0x42, 0x8c, 0xff, 0x34, 0x12, 0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0xbc,
                0x9a
            ]
        );
    }

    #[test]
    fn encode_ver2_none() {
        let frame = Frame {
            header: Header {
                frame_type: FrameType::MacCommand,
                security: Security::None,
                frame_pending: false,
                ack_request: true,
                pan_id_compress: false,
                version: FrameVersion::Ieee802154,
                destination: None,
                source: Some(Address::Short(PanId(0x1234), ShortAddress(0x9abc))),
                seq: 0xff,
            },
            content: FrameContent::Command(command::Command::DataRequest),
            payload: &[],
            footer: [0x00, 0x00],
        };
        let mut buf = [0u8; 32];
        let mut len = 0usize;
        buf.write(&mut len, frame).unwrap();
        assert_eq!(len, 8);
        assert_eq!(buf[..len], [0x23, 0xa0, 0xff, 0x34, 0x12, 0xbc, 0x9a, 0x04]);
    }
}
