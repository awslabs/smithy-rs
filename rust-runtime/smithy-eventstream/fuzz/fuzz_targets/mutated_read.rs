/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

#![no_main]

use bytes::{Buf, BufMut};
use crc32fast::Hasher as Crc;
use libfuzzer_sys::{fuzz_mutator, fuzz_target};
use smithy_eventstream::frame::{Header, HeaderValue, Message};
use smithy_types::Instant;

fn mutate(data: &mut [u8], size: usize, max_size: usize) -> usize {
    let input = &mut &data[..size];
    let message = {
        let result = Message::read_from(input);
        if result.is_err() || result.as_ref().unwrap().is_none() {
            Message::new(&b"some payload"[..])
                .add_header(Header::new("true", HeaderValue::Bool(true)))
                .add_header(Header::new("false", HeaderValue::Bool(false)))
                .add_header(Header::new("byte", HeaderValue::Byte(50)))
                .add_header(Header::new("short", HeaderValue::Int16(20_000)))
                .add_header(Header::new("int", HeaderValue::Int32(500_000)))
                .add_header(Header::new("long", HeaderValue::Int64(50_000_000_000)))
                .add_header(Header::new(
                    "bytes",
                    HeaderValue::ByteArray((&b"some bytes"[..]).into()),
                ))
                .add_header(Header::new(
                    "str",
                    HeaderValue::String((&b"some str"[..]).into()),
                ))
                .add_header(Header::new(
                    "time",
                    HeaderValue::Timestamp(Instant::from_epoch_seconds(5_000_000_000)),
                ))
                .add_header(Header::new(
                    "uuid",
                    HeaderValue::Uuid(0xb79bc914_de21_4e13_b8b2_bc47e85b7f0b),
                ))
        } else {
            result.unwrap().unwrap()
        }
    };

    let mut bytes = Vec::new();
    message.write_to(&mut bytes).unwrap();

    let headers_len = (&bytes[4..8]).get_u32();
    let non_header_len = bytes.len() - headers_len as usize;
    let max_header_len = max_size - non_header_len;
    let mut headers = (&bytes[12..(12 + headers_len as usize)]).to_vec();
    headers.resize(max_header_len, 0);
    let new_header_len =
        libfuzzer_sys::fuzzer_mutate(&mut headers, headers_len as usize, max_header_len);

    let mut mutated = Vec::<u8>::new();
    mutated.put_u32((new_header_len + non_header_len) as u32);
    mutated.put_u32(new_header_len as u32);
    mutated.put_u32(crc(&mutated));
    mutated.put_slice(&headers[..new_header_len]);
    mutated.put_slice(message.payload());
    mutated.put_u32(crc(&mutated));

    data[..mutated.len()].copy_from_slice(&mutated);
    mutated.len()
}

fuzz_mutator!(
    |data: &mut [u8], size: usize, max_size: usize, _seed: u32| { mutate(data, size, max_size) }
);

fuzz_target!(|data: &[u8]| {
    let mut message = data;
    let _ = Message::read_from(&mut message);
});

fn crc(input: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(input);
    crc.finalize()
}
