use super::types::{ByteRep, Record};
use crate::error::{Error, ErrorKind};
use crate::globals::GLOBALS;
use heed::types::Bytes;
use heed::{Database, RoTxn, RwTxn};

pub trait Table {
    type Item: Record;

    fn lmdb_name() -> &'static str;

    /// Get the heed database
    fn db() -> Result<Database<Bytes, Bytes>, Error>;

    /// Whether or not 'new' is implemented
    /// (some tables can't do 'new', such as Event, and any calls that need it
    /// will return an error)
    fn newable() -> bool;

    /// Number of records
    #[allow(dead_code)]
    fn num_records() -> Result<u64, Error> {
        let txn = GLOBALS.storage.env().read_txn()?;
        Ok(Self::db()?.len(&txn)?)
    }

    /// Write a record
    /// (it needs to be mutable for possible stabilization)
    #[allow(dead_code)]
    fn write_record(record: &mut Self::Item, wtxn: Option<&mut RwTxn<'_>>) -> Result<(), Error> {
        record.stabilize();
        let keybytes = record.key().to_bytes()?;
        let valbytes = record.to_bytes()?;
        let f = |txn: &mut RwTxn<'_>| -> Result<(), Error> {
            Self::db()?.put(txn, &keybytes, &valbytes)?;
            Ok(())
        };

        match wtxn {
            Some(txn) => f(txn),
            None => {
                let mut txn = GLOBALS.storage.get_write_txn()?;
                let result = f(&mut txn);
                txn.commit()?;
                result
            }
        }
    }

    /// Write a default record if missing
    #[allow(dead_code)]
    fn create_record_if_missing(
        key: <Self::Item as Record>::Key,
        wtxn: Option<&mut RwTxn<'_>>,
    ) -> Result<(), Error> {
        if !Self::newable() {
            return Err(ErrorKind::RecordIsNotNewable.into());
        }

        let keybytes = key.to_bytes()?;
        let f = |txn: &mut RwTxn<'_>| -> Result<(), Error> {
            if Self::db()?.get(txn, &keybytes)?.is_none() {
                let mut record = <Self::Item as Record>::new(key);
                record.stabilize();
                let valbytes = record.to_bytes()?;
                Self::db()?.put(txn, &keybytes, &valbytes)?;
            }
            Ok(())
        };

        match wtxn {
            Some(txn) => f(txn),
            None => {
                let mut txn = GLOBALS.storage.get_write_txn()?;
                let result = f(&mut txn);
                txn.commit()?;
                result
            }
        }
    }

    /// Check if a record exists
    #[allow(dead_code)]
    fn has_record(
        key: <Self::Item as Record>::Key,
        rtxn: Option<&RoTxn<'_>>,
    ) -> Result<bool, Error> {
        let keybytes = key.to_bytes()?;
        let f = |txn: &RoTxn<'_>| -> Result<bool, Error> {
            Ok(Self::db()?.get(txn, &keybytes)?.is_some())
        };

        match rtxn {
            Some(txn) => f(txn),
            None => {
                let txn = GLOBALS.storage.get_read_txn()?;
                f(&txn)
            }
        }
    }

    /// Read a record
    #[allow(dead_code)]
    fn read_record(
        key: <Self::Item as Record>::Key,
        rtxn: Option<&RoTxn<'_>>,
    ) -> Result<Option<Self::Item>, Error> {
        let keybytes = key.to_bytes()?;
        let f = |txn: &RoTxn<'_>| -> Result<Option<Self::Item>, Error> {
            let valbytes = Self::db()?.get(txn, &keybytes)?;
            Ok(match valbytes {
                None => None,
                Some(valbytes) => Some(<Self::Item>::from_bytes(valbytes)?),
            })
        };

        match rtxn {
            Some(txn) => f(txn),
            None => {
                let txn = GLOBALS.storage.get_read_txn()?;
                f(&txn)
            }
        }
    }

    /// Read a record or create a new one (and store it)
    ///
    /// Will error if the Record is not newable
    #[allow(dead_code)]
    fn read_or_create_record(
        key: <Self::Item as Record>::Key,
        wtxn: Option<&mut RwTxn<'_>>,
    ) -> Result<Self::Item, Error> {
        if !Self::newable() {
            return Err(ErrorKind::RecordIsNotNewable.into());
        }

        let keybytes = key.to_bytes()?;
        let f = |txn: &mut RwTxn<'_>| -> Result<Self::Item, Error> {
            let valbytes = Self::db()?.get(txn, &keybytes)?;
            Ok(match valbytes {
                None => {
                    let mut record = <Self::Item as Record>::new(key);
                    record.stabilize();
                    let valbytes = record.to_bytes()?;
                    Self::db()?.put(txn, &keybytes, &valbytes)?;
                    record
                }
                Some(valbytes) => <Self::Item>::from_bytes(valbytes)?,
            })
        };

        match wtxn {
            Some(txn) => f(txn),
            None => {
                let mut txn = GLOBALS.storage.get_write_txn()?;
                let result = f(&mut txn);
                txn.commit()?;
                result
            }
        }
    }

    /// filter_records
    fn filter_records<F>(f: F, rtxn: Option<&RoTxn<'_>>) -> Result<Vec<Self::Item>, Error>
    where
        F: Fn(&Self::Item) -> bool,
    {
        let f = |txn: &RoTxn<'_>| -> Result<Vec<Self::Item>, Error> {
            let iter = Self::db()?.iter(txn)?;
            let mut output: Vec<Self::Item> = Vec::new();
            for result in iter {
                let (_keybytes, valbytes) = result?;
                let record = <Self::Item>::from_bytes(valbytes)?;
                if f(&record) {
                    output.push(record);
                }
            }
            Ok(output)
        };

        match rtxn {
            Some(txn) => f(txn),
            None => {
                let txn = GLOBALS.storage.get_read_txn()?;
                f(&txn)
            }
        }
    }

    /// Modify a record in the database if it exists; returns false if not found
    #[allow(dead_code)]
    fn modify_if_exists<M>(
        key: <Self::Item as Record>::Key,
        mut modify: M,
        wtxn: Option<&mut RwTxn<'_>>,
    ) -> Result<bool, Error>
    where
        M: FnMut(&mut Self::Item),
    {
        let mut f = |txn: &mut RwTxn<'_>| -> Result<bool, Error> {
            let keybytes = key.to_bytes()?;
            let valbytes = Self::db()?.get(txn, &keybytes)?;
            let mut record = match valbytes {
                Some(valbytes) => Self::Item::from_bytes(valbytes)?,
                None => return Ok(false),
            };
            modify(&mut record);
            record.stabilize();
            let valbytes = record.to_bytes()?;
            Self::db()?.put(txn, &keybytes, &valbytes)?;
            Ok(true)
        };

        match wtxn {
            Some(txn) => f(txn),
            None => {
                let mut txn = GLOBALS.storage.get_write_txn()?;
                let result = f(&mut txn);
                txn.commit()?;
                result
            }
        }
    }

    /// Modify a record in the database; create first if missing
    ///
    /// Will error if the Record is not newable (see modify_if_exists)
    #[allow(dead_code)]
    fn modify<M>(
        key: <Self::Item as Record>::Key,
        mut modify: M,
        wtxn: Option<&mut RwTxn<'_>>,
    ) -> Result<(), Error>
    where
        M: FnMut(&mut Self::Item),
    {
        if !Self::newable() {
            return Err(ErrorKind::RecordIsNotNewable.into());
        }

        let mut f = |txn: &mut RwTxn<'_>| -> Result<(), Error> {
            let keybytes = key.to_bytes()?;
            let valbytes = Self::db()?.get(txn, &keybytes)?;
            let mut record = match valbytes {
                Some(valbytes) => Self::Item::from_bytes(valbytes)?,
                None => Self::Item::new(key),
            };
            modify(&mut record);
            record.stabilize();
            let valbytes = record.to_bytes()?;
            Self::db()?.put(txn, &keybytes, &valbytes)?;
            Ok(())
        };

        match wtxn {
            Some(txn) => f(txn),
            None => {
                let mut txn = GLOBALS.storage.get_write_txn()?;
                let result = f(&mut txn);
                txn.commit()?;
                result
            }
        }
    }

    /// Modify all matching records in the database
    #[allow(dead_code)]
    fn filter_modify<F, M>(f: F, mut modify: M, wtxn: Option<&mut RwTxn<'_>>) -> Result<(), Error>
    where
        F: Fn(&Self::Item) -> bool,
        M: FnMut(&mut Self::Item),
    {
        let mut f = |txn: &mut RwTxn<'_>| -> Result<(), Error> {
            let mut iter = Self::db()?.iter_mut(txn)?;
            while let Some(result) = iter.next() {
                let (keybytes, valbytes) = result?;
                let mut record = Self::Item::from_bytes(valbytes)?;
                if f(&record) {
                    modify(&mut record);
                    record.stabilize();
                    let valbytes = record.to_bytes()?;
                    let keybytes = keybytes.to_owned();
                    unsafe {
                        iter.put_current(&keybytes, &valbytes)?;
                    }
                }
            }
            Ok(())
        };

        match wtxn {
            Some(txn) => f(txn),
            None => {
                let mut txn = GLOBALS.storage.get_write_txn()?;
                let result = f(&mut txn);
                txn.commit()?;
                result
            }
        }
    }
}
