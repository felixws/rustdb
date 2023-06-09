#![allow(dead_code)]
use crate::{memtable::MemTable, sstable::sstable::SSTable, wal::wal::WAL};
use std::{io, path::PathBuf};

use std::path::Path;

use super::entry::Entry;

struct Database {
    memtable: MemTable,
    wal: WAL,
    sstables: Vec<PathBuf>,
}

impl Database {
    pub fn new(dir: &Path) -> io::Result<Database> {
        let wal = WAL::new(dir)?;
        let memtable = MemTable::new();
        let sstables: Vec<PathBuf> = Vec::new();
        Ok(Database {
            wal,
            memtable,
            sstables,
        })
    }

    pub fn set(&mut self, key: &[u8], value: &[u8], timestamp: u128) -> Result<(), std::io::Error> {
        self.memtable.set(key, value, timestamp);
        self.wal.set(key, value, timestamp)
    }

    pub fn delete(&mut self, key: &[u8], timestamp: u128) -> Result<(), std::io::Error> {
        self.memtable.delete(key, timestamp);
        self.wal.delete(key, timestamp)
    }

    pub fn get(&self, key: &[u8]) -> Option<Entry> {
        if let Some(entry) = self.memtable.get(key) {
            Some(entry.clone())
        } else {
            for path in self.sstables.iter() {
                let sstable = SSTable::from_path(path).ok().unwrap();
                if let Some(entry) = sstable.get(key).ok().unwrap() {
                    return Some(entry.clone());
                }
            }
            None
        }
    }
    fn flush(&mut self, dir: &Path) -> io::Result<()> {
        let mut sstable = SSTable::new(dir)?;
        for entry in &self.memtable {
            let entry = Entry {
                key: entry.key,
                value: entry.value,
                deleted: entry.deleted,
                timestamp: entry.timestamp,
            };
            sstable.write(&entry)?;
        }
        sstable.flush()?;
        self.sstables.push(sstable.path);
        self.memtable = MemTable::new();
        self.wal = WAL::new(&self.wal.path.parent().unwrap())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_path() -> PathBuf {
        PathBuf::from("data")
    }

    fn create_database() -> Database {
        let path = create_path();
        Database::new(&path).unwrap()
    }

    fn create_entry() -> Entry {
        Entry {
            key: vec![1, 2, 3],
            value: Some(vec![9]),
            timestamp: 1,
            deleted: false,
        }
    }

    #[test]
    fn test_read_after_write() {
        let mut db = create_database();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        let db_entry = db.get(&entry.key.as_slice()).unwrap();
        assert_eq!(&entry.value.unwrap(), db_entry.value.as_ref().unwrap());
    }

    #[test]
    fn test_sstable_path_is_added_on_flush() {
        let mut db = create_database();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        let path = create_path();
        db.flush(&path).ok();
        let sstables = &db.sstables;
        assert_eq!(sstables.len(), 1);
    }

    #[test]
    fn test_memtable_is_empty_after_flush() {
        let mut db = create_database();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        let path = create_path();
        db.flush(&path).ok();
        assert_eq!(db.memtable.size, 0);
    }

    #[test]
    fn test_wal_is_empty_after_flush() {
        let mut db = create_database();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        let path = create_path();
        db.flush(&path).ok();
        assert_eq!(db.wal.into_iter().count(), 0);
    }

    #[test]
    fn test_items_from_database_and_sstable_are_identical() {
        let mut db = create_database();
        let path = create_path();
        let mut sstable = SSTable::new(&path).unwrap();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        write_entry_to_sstable(&mut sstable, &entry);
        sstable.flush().ok();
        db.flush(&path).ok();
        let item = sstable.get(entry.key.as_slice()).unwrap();
        assert_eq!(entry.value.unwrap(), item.unwrap().value.unwrap());
    }

    #[test]
    fn test_scan_sstable_for_entries_when_not_found_in_memtable() {
        let mut db = create_database();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        let path = create_path();
        db.flush(&path).ok();
        let return_value = db.get(&entry.key.as_slice());
        assert!(return_value.is_some());
    }

    #[test]
    fn test_scanning_sstables_for_non_existent_entry_returns_none() {
        let mut db = create_database();
        let entry = create_entry();
        write_entry_to_db(&mut db, &entry);
        let path = create_path();
        db.flush(&path).ok();
        let key = vec![0, 0, 0, 0];
        assert_ne!(key.as_slice(), entry.key.as_slice());
        assert!(db.get(key.as_slice()).is_none());
    }

    fn write_entry_to_sstable(sstable: &mut SSTable, entry: &Entry) {
        let entry = Entry {
            key: entry.key.clone(),
            value: entry.value.clone(),
            deleted: entry.deleted,
            timestamp: entry.timestamp,
        };
        sstable.write(&entry).ok();
    }

    fn write_entry_to_db(db: &mut Database, entry: &Entry) {
        db.set(
            entry.key.as_slice(),
            entry.value.as_ref().unwrap().as_slice(),
            entry.timestamp,
        )
        .ok();
    }
}
