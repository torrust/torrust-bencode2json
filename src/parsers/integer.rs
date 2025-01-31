//! Bencoded integer parser.
//!
//! It reads bencoded bytes from the input and writes JSON bytes to the output.
use std::io::{self, Read};

use crate::rw::{byte_reader::ByteReader, writer::Writer};

use super::{
    error::{Error, ReadContext, WriteContext},
    BENCODE_END_INTEGER,
};

/// The current state parsing the integer.
#[derive(PartialEq)]
#[allow(clippy::enum_variant_names)]
enum StateExpecting {
    Start,          // S
    DigitOrSign,    // DoS
    DigitAfterSign, // DaS
    DigitOrEnd,     // DoE
}

/// It parses an integer bencoded value.
///
/// # Errors
///
/// Will return an error if it can't read from the input or write to the
/// output.
///
/// # Panics
///
/// Will panic if we reach the end of the input without completing the integer
/// (without reaching the end of the integer `e`).
pub fn parse<R: Read, W: Writer>(reader: &mut ByteReader<R>, writer: &mut W) -> Result<(), Error> {
    let mut state = StateExpecting::Start;
    let mut first_digit_is_zero = false;

    loop {
        let byte = next_byte(reader, writer)?;

        let char = byte as char;

        state = match state {
            StateExpecting::Start => {
                // Discard the 'i' byte
                StateExpecting::DigitOrSign
            }
            StateExpecting::DigitOrSign => {
                if char == '-' {
                    writer.write_byte(byte)?;

                    StateExpecting::DigitAfterSign
                } else if char.is_ascii_digit() {
                    writer.write_byte(byte)?;

                    if char == '0' {
                        first_digit_is_zero = true;
                    }

                    StateExpecting::DigitOrEnd
                } else {
                    return Err(Error::UnexpectedByteParsingInteger(
                        ReadContext {
                            byte: Some(byte),
                            pos: reader.input_byte_counter(),
                            latest_bytes: reader.captured_bytes(),
                        },
                        WriteContext {
                            byte: Some(byte),
                            pos: writer.output_byte_counter(),
                            latest_bytes: writer.captured_bytes(),
                        },
                    ));
                }
            }
            StateExpecting::DigitAfterSign => {
                if char.is_ascii_digit() {
                    writer.write_byte(byte)?;

                    if char == '0' {
                        first_digit_is_zero = true;
                    }

                    StateExpecting::DigitOrEnd
                } else {
                    return Err(Error::UnexpectedByteParsingInteger(
                        ReadContext {
                            byte: Some(byte),
                            pos: reader.input_byte_counter(),
                            latest_bytes: reader.captured_bytes(),
                        },
                        WriteContext {
                            byte: Some(byte),
                            pos: writer.output_byte_counter(),
                            latest_bytes: writer.captured_bytes(),
                        },
                    ));
                }
            }
            StateExpecting::DigitOrEnd => {
                if char.is_ascii_digit() {
                    writer.write_byte(byte)?;

                    if char == '0' && first_digit_is_zero {
                        return Err(Error::LeadingZerosInIntegersNotAllowed(
                            ReadContext {
                                byte: Some(byte),
                                pos: reader.input_byte_counter(),
                                latest_bytes: reader.captured_bytes(),
                            },
                            WriteContext {
                                byte: Some(byte),
                                pos: writer.output_byte_counter(),
                                latest_bytes: writer.captured_bytes(),
                            },
                        ));
                    }

                    StateExpecting::DigitOrEnd
                } else if byte == BENCODE_END_INTEGER {
                    return Ok(());
                } else {
                    return Err(Error::UnexpectedByteParsingInteger(
                        ReadContext {
                            byte: Some(byte),
                            pos: reader.input_byte_counter(),
                            latest_bytes: reader.captured_bytes(),
                        },
                        WriteContext {
                            byte: Some(byte),
                            pos: writer.output_byte_counter(),
                            latest_bytes: writer.captured_bytes(),
                        },
                    ));
                }
            }
        };
    }
}

/// It reads the next byte from the input.
///
/// # Errors
///
/// Will return an error if the end of input was reached.
fn next_byte<R: Read, W: Writer>(reader: &mut ByteReader<R>, writer: &W) -> Result<u8, Error> {
    match reader.read_byte() {
        Ok(byte) => Ok(byte),
        Err(err) => {
            if err.kind() == io::ErrorKind::UnexpectedEof {
                return Err(Error::UnexpectedEndOfInputParsingInteger(
                    ReadContext {
                        byte: None,
                        pos: reader.input_byte_counter(),
                        latest_bytes: reader.captured_bytes(),
                    },
                    WriteContext {
                        byte: None,
                        pos: writer.output_byte_counter(),
                        latest_bytes: writer.captured_bytes(),
                    },
                ));
            }
            Err(err.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        parsers::{error::Error, integer::parse},
        rw::{byte_reader::ByteReader, string_writer::StringWriter},
    };

    fn bencode_to_json_unchecked(input_buffer: &[u8]) -> String {
        let mut output = String::new();

        parse_bencode(input_buffer, &mut output).expect("Bencode to JSON conversion failed");

        output
    }

    fn try_bencode_to_json(input_buffer: &[u8]) -> Result<String, Error> {
        let mut output = String::new();

        match parse_bencode(input_buffer, &mut output) {
            Ok(()) => Ok(output),
            Err(err) => Err(err),
        }
    }

    fn parse_bencode(input_buffer: &[u8], output: &mut String) -> Result<(), Error> {
        let mut reader = ByteReader::new(input_buffer);

        let mut writer = StringWriter::new(output);

        parse(&mut reader, &mut writer)
    }

    mod for_helpers {
        use crate::parsers::integer::tests::try_bencode_to_json;

        #[test]
        fn bencode_to_json_wrapper_succeeds() {
            assert_eq!(try_bencode_to_json(b"i0e").unwrap(), "0".to_string());
        }

        #[test]
        fn bencode_to_json_wrapper_fails() {
            assert!(try_bencode_to_json(b"i").is_err());
        }
    }

    #[test]
    fn zero() {
        assert_eq!(bencode_to_json_unchecked(b"i0e"), "0".to_string());
    }

    #[test]
    fn one_digit_integer() {
        assert_eq!(bencode_to_json_unchecked(b"i1e"), "1".to_string());
    }

    #[test]
    fn two_digits_integer() {
        assert_eq!(bencode_to_json_unchecked(b"i42e"), "42".to_string());
    }

    #[test]
    fn negative_integer() {
        assert_eq!(bencode_to_json_unchecked(b"i-1e"), "-1".to_string());
    }

    mod it_should_fail {
        use std::io::{self, Read};

        use crate::{
            parsers::{
                error::Error,
                integer::{parse, tests::try_bencode_to_json},
            },
            rw::{byte_reader::ByteReader, string_writer::StringWriter},
        };

        #[test]
        fn when_it_cannot_read_more_bytes_from_input() {
            let unfinished_int = b"i42";

            let result = try_bencode_to_json(unfinished_int);

            assert!(matches!(
                result,
                Err(Error::UnexpectedEndOfInputParsingInteger { .. })
            ));
        }

        #[test]
        fn when_it_finds_an_invalid_byte() {
            let int_with_invalid_byte = b"iae";

            let result = try_bencode_to_json(int_with_invalid_byte);

            assert!(matches!(
                result,
                Err(Error::UnexpectedByteParsingInteger { .. })
            ));
        }

        #[test]
        fn when_it_finds_leading_zeros() {
            // Leading zeros are not allowed.Only the zero integer can start with zero.

            let int_with_invalid_byte = b"i00e";

            let result = try_bencode_to_json(int_with_invalid_byte);

            assert!(matches!(
                result,
                Err(Error::LeadingZerosInIntegersNotAllowed { .. })
            ));
        }

        #[test]
        fn when_it_finds_leading_zeros_in_a_negative_integer() {
            // Leading zeros are not allowed.Only the zero integer can start with zero.

            let int_with_invalid_byte = b"i-00e";

            let result = try_bencode_to_json(int_with_invalid_byte);

            assert!(matches!(
                result,
                Err(Error::LeadingZerosInIntegersNotAllowed { .. })
            ));
        }

        mod when_it_receives_a_unexpected_byte {
            use crate::parsers::{error::Error, integer::tests::try_bencode_to_json};

            #[test]
            fn while_expecting_a_digit_or_sign() {
                let int_with_invalid_byte = b"ia";

                let result = try_bencode_to_json(int_with_invalid_byte);

                assert!(matches!(
                    result,
                    Err(Error::UnexpectedByteParsingInteger { .. })
                ));
            }

            #[test]
            fn while_expecting_digit_after_the_sign() {
                let int_with_invalid_byte = b"i-a";

                let result = try_bencode_to_json(int_with_invalid_byte);

                assert!(matches!(
                    result,
                    Err(Error::UnexpectedByteParsingInteger { .. })
                ));
            }

            #[test]
            fn while_expecting_digit_or_end() {
                let int_with_invalid_byte = b"i-1a";

                let result = try_bencode_to_json(int_with_invalid_byte);

                assert!(matches!(
                    result,
                    Err(Error::UnexpectedByteParsingInteger { .. })
                ));
            }
        }

        #[test]
        fn when_it_receives_a_non_eof_io_error() {
            struct FaultyReader;

            impl Read for FaultyReader {
                fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
                    Err(io::Error::new(
                        io::ErrorKind::PermissionDenied,
                        "Permission denied",
                    ))
                }
            }

            let mut reader = ByteReader::new(FaultyReader);

            let mut output = String::new();
            let mut writer = StringWriter::new(&mut output);

            let result = parse(&mut reader, &mut writer);

            assert!(matches!(result, Err(Error::Io(_))));
        }
    }
}
