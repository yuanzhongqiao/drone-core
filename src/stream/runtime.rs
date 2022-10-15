#![cfg_attr(feature = "std", allow(unused_imports, unused_variables))]

use super::{STREAM_CORE0_BUF_BASE, STREAM_CORE0_BUF_END};
use crate::platform::Interrupts;
use core::ptr;
use drone_stream::{Runtime, HEADER_LENGTH};

const DEFAULT_TRANSACTION_LENGTH: u8 = 64;

pub trait LocalRuntime {
    fn is_enabled(&self, stream: u8) -> bool;

    unsafe fn write_bytes(&mut self, stream: u8, buffer: *const u8, length: usize);

    unsafe fn write_transaction(&mut self, stream: u8, buffer: *const u8, length: u8);
}

impl LocalRuntime for Runtime {
    fn is_enabled(&self, stream: u8) -> bool {
        unsafe { ptr::addr_of!(self.enable_mask).read_volatile() & 1 << stream != 0 }
    }

    unsafe fn write_bytes(&mut self, stream: u8, mut buffer: *const u8, mut length: usize) {
        while length > usize::from(DEFAULT_TRANSACTION_LENGTH) {
            length -= usize::from(DEFAULT_TRANSACTION_LENGTH);
            unsafe { self.write_transaction(stream, buffer, DEFAULT_TRANSACTION_LENGTH) };
            buffer = unsafe { buffer.add(usize::from(DEFAULT_TRANSACTION_LENGTH)) };
        }
        if length > 0 {
            unsafe { self.write_transaction(stream, buffer, length as u8) };
        }
    }

    unsafe fn write_transaction(&mut self, stream: u8, buffer: *const u8, length: u8) {
        #[cfg(feature = "std")]
        return unimplemented!();
        #[cfg(not(feature = "std"))]
        unsafe {
            let buffer_size =
                (STREAM_CORE0_BUF_END.get() as usize - STREAM_CORE0_BUF_BASE.get() as usize) as u32;
            loop {
                let _critical = Interrupts::pause();
                let read_cursor = ptr::addr_of!(self.read_cursor).read_volatile();
                let write_cursor = ptr::addr_of!(self.write_cursor).read_volatile();
                let wrapped = write_cursor >= read_cursor;
                let available = if wrapped { buffer_size } else { read_cursor } - write_cursor;
                let frame_length = u32::from(length) + HEADER_LENGTH;
                let cursor = STREAM_CORE0_BUF_BASE.get().add(write_cursor as usize);
                if available >= frame_length {
                    let mut next_write_cursor = write_cursor + frame_length;
                    if next_write_cursor == buffer_size {
                        next_write_cursor = 0;
                    }
                    if next_write_cursor == read_cursor {
                        continue;
                    }
                    *cursor = stream;
                    *cursor.add(1) = length;
                    cursor.add(2).copy_from_nonoverlapping(buffer, usize::from(length));
                    ptr::addr_of_mut!(self.write_cursor).write_volatile(next_write_cursor);
                    break;
                }
                if wrapped {
                    if available > HEADER_LENGTH {
                        *cursor = 0xFF;
                        *cursor.add(1) = (available - HEADER_LENGTH) as u8;
                    }
                    ptr::addr_of_mut!(self.write_cursor).write_volatile(0);
                }
            }
        }
    }
}
