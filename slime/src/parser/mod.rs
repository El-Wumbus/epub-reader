pub mod ini;

pub trait UnParser
where
    Self: Iterator, {
    fn serialize<W: std::io::Write>(
        &mut self,
        to: &mut W,
    ) -> std::io::Result<()>;

    fn serialize_into_bytes(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        self.serialize(&mut std::io::Cursor::new(buf))
    }

    fn serialize_to_bytes(&mut self) -> std::io::Result<Vec<u8>> {
        let mut buf = std::io::Cursor::new(vec![]);
        self.serialize(&mut buf)?;
        Ok(buf.into_inner())
    }

    fn serialize_to_string(&mut self) -> std::io::Result<String> {
        String::from_utf8(self.serialize_to_bytes()?)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
