use std::{fs, io, ops::Range, path::PathBuf};

use crate::export_import_content::{
    DiagramEmbedding, SupportedEmbeddingType, SupportedFileType, scan_for_embeddings,
};

pub enum FileScanError {
    IO(io::Error),
}

#[derive(Clone)]
pub struct Backlink {
    pub source_path: PathBuf,
    pub file_path: PathBuf,
    pub embedding_type: SupportedEmbeddingType,
    pub buf_position: Range<usize>,
}

#[derive(Clone)]
pub enum DiagramScanItem {
    NativeSource(PathBuf),
    Backlink(Backlink),
}

pub fn io_scan_path_for_kontur_files(path: PathBuf) -> Result<Vec<DiagramScanItem>, FileScanError> {
    let mut items: Vec<DiagramScanItem> = Vec::new();
    if path.is_dir() {
        for dir_entry in fs::read_dir(path)? {
            let dir_entry = dir_entry?;
            let is_file = dir_entry.metadata()?.is_file();
            let entry_path = dir_entry.path();

            if is_file
                && let Some(ext) = entry_path.extension()
                && let Some(file_type) = SupportedFileType::try_parse(&ext.to_string_lossy())
            {
                match file_type {
                    SupportedFileType::Native => {
                        items.push(DiagramScanItem::NativeSource(entry_path));
                    }
                    SupportedFileType::Embedded(embedded) => {
                        let file_str = fs::read_to_string(entry_path.clone())?;
                        items.extend(scan_for_embeddings(&file_str, embedded).into_iter().map(
                            |DiagramEmbedding {
                                 source,
                                 buf_position,
                                 ..
                             }| {
                                DiagramScanItem::Backlink(Backlink {
                                    source_path: source,
                                    file_path: entry_path.clone(),
                                    embedding_type: embedded,
                                    buf_position,
                                })
                            },
                        ));
                    }
                }
            }
        }
    }
    todo!()
}

impl From<io::Error> for FileScanError {
    fn from(value: io::Error) -> Self {
        FileScanError::IO(value)
    }
}
