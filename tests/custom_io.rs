use libchdman_rs::Chd;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;

struct MemoryIo {
    data: Arc<Mutex<Vec<u8>>>,
    pos: u64,
}

impl Read for MemoryIo {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self.data.lock().unwrap();
        let offset = self.pos as usize;
        if offset >= data.len() {
            return Ok(0);
        }
        let available = data.len() - offset;
        let to_read = std::cmp::min(available, buf.len());
        buf[..to_read].copy_from_slice(&data[offset..offset + to_read]);
        self.pos += to_read as u64;
        Ok(to_read)
    }
}

impl Write for MemoryIo {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut data = self.data.lock().unwrap();
        let offset = self.pos as usize;
        let end = offset + buf.len();
        if end > data.len() {
            data.resize(end, 0);
        }
        data[offset..end].copy_from_slice(buf);
        self.pos += buf.len() as u64;
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Seek for MemoryIo {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let mut data_len = self.data.lock().unwrap().len() as u64;
        match pos {
            SeekFrom::Start(p) => self.pos = p,
            SeekFrom::End(p) => self.pos = (data_len as i64 + p) as u64,
            SeekFrom::Current(p) => self.pos = (self.pos as i64 + p) as u64,
        }
        Ok(self.pos)
    }
}

#[test]
fn test_custom_io() {
    let temp_file = NamedTempFile::new().unwrap();
    let chd_path = temp_file.path().to_str().unwrap();

    // 1. Create a CHD on disk first
    {
        let mut _chd = Chd::create(chd_path, 1024 * 1024, 4096, 512, [0, 0, 0, 0])
            .expect("Failed to create CHD");
    }

    // 2. Read it into memory
    let mut file_data = Vec::new();
    std::fs::File::open(chd_path)
        .unwrap()
        .read_to_end(&mut file_data)
        .unwrap();
    let memory_data = Arc::new(Mutex::new(file_data));

    // 3. Open it via custom IO
    let io = MemoryIo {
        data: memory_data.clone(),
        pos: 0,
    };
    let chd = Chd::open_custom(io, false, None).expect("Failed to open CHD via custom IO");

    assert_eq!(chd.logical_bytes(), 1024 * 1024);
    assert_eq!(chd.hunk_bytes(), 4096);
}
