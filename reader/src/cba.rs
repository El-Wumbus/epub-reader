use std::io::{BufRead, BufReader, Read};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Unknown file type: failed to detect file type")]
    UnknownFileType,
    #[error("Invalid File Type: detected file type is unsupported: \"{0}\"")]
    InvalidFileType(mime::Mime),
    #[error("Index Out of Bounds: provided index ({0}) is out of bounds")]
    IndexOutOfBounds(usize),
}

enum InnerArchive {
    Zip(zip::ZipArchive<BufReader<std::fs::File>>),
}

pub struct CBAReader {
    inner: InnerArchive,
    /// An ordered list of indices
    indices: Vec<usize>,

    position: usize,
}

impl CBAReader {
    pub fn read(path: &Path) -> Result<Self, Error> {
        let mut file = BufReader::new(std::fs::File::open(path)?);

        let buffer = file.fill_buf().unwrap();
        let kind = infer::get(buffer).ok_or(Error::UnknownFileType)?;
        // We don't call `file.consume` here because we do want this data
        // returned later.

        let mime: mime::Mime = kind
            .mime_type()
            .parse()
            .map_err(|_| Error::UnknownFileType)?;
        match (mime.type_().as_str(), mime.subtype().as_str()) {
            ("application", "zip") => {
                let mut zip = zip::ZipArchive::new(file)?;

                let mut indices = Vec::with_capacity(zip.len());
                for i in 0..zip.len() {
                    let name = zip.by_index(i)?.name().to_string();
                    let mime = {
                        let name: &Path = name.as_ref();
                        let Some(ext) =
                            name.extension().and_then(|x| x.to_str())
                        else {
                            continue;
                        };
                        let Some(mime) = mime_guess::from_ext(ext).first()
                        else {
                            continue;
                        };
                        mime
                    };
                    if mime.type_() == mime::IMAGE {
                        indices.push((i, name));
                    }
                }

                indices.sort_by(|(_, left), (_, right)| left.cmp(right));
                let indices =
                    indices.into_iter().map(|(i, _)| i).collect::<Vec<_>>();

                dbg!(&indices);
                let inner = InnerArchive::Zip(zip);
                Ok(Self {
                    inner,
                    indices,
                    position: 0,
                })
            }
            ("application", "vnd.rar") => {
                todo!("Process RAR");
            }
            _invalid => Err(Error::InvalidFileType(mime)),
        }
    }

    pub fn page_count(&self) -> usize {
        self.indices.len()
    }

    pub fn get_current_page(&self) -> usize {
        self.position
    }

    /// Try to seek to `pos`, returning the previous position if successful.
    pub fn set_current_page(&mut self, pos: usize) -> Option<usize> {
        if pos >= self.indices.len() {
            return None;
        }
        let old = self.position;
        self.position = pos;
        Some(old)
    }

    /// Returns the data for the page at `pos` as an image as well as the type
    /// of image it is.
    pub fn page(&mut self, pos: usize) -> Result<(Vec<u8>, mime::Mime), Error> {
        let mut contents = vec![];
        let mime: mime::Mime;

        let idx = *self.indices.get(pos).ok_or(Error::IndexOutOfBounds(pos))?;
        match &mut self.inner {
            InnerArchive::Zip(zip) => {
                let mut file = zip.by_index(idx)?;
                mime = mime_guess::from_path(file.name()).first().unwrap();
                file.read_to_end(&mut contents)?;
            }
        }
        Ok((contents, mime))
    }
}
