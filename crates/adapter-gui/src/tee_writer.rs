//! Responsibility: duplicates a byte stream into two writers (#1060).

use std::io::{self, Write};

pub struct TeeWriter<A: Write, B: Write> {
    a: A,
    b: B,
}

impl<A: Write, B: Write> TeeWriter<A, B> {
    pub fn new(a: A, b: B) -> Self {
        Self { a, b }
    }

    pub fn into_inner(self) -> (A, B) {
        (self.a, self.b)
    }
}

impl<A: Write, B: Write> Write for TeeWriter<A, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // A failing sink must not silence the other one.
        let _ = self.a.write_all(buf);
        let _ = self.b.write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let _ = self.a.flush();
        let _ = self.b.flush();
        Ok(())
    }
}
