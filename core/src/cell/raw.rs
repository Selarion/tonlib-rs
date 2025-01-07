use std::io::{Cursor, SeekFrom};
use std::ptr::read;
use bitstream_io::{BigEndian, BitWrite, BitWriter, ByteRead, ByteReader, Numeric};
use crc::Crc;
use lazy_static::lazy_static;
use num_traits::ToPrimitive;
use crate::cell::level_mask::LevelMask;
use crate::cell::{MapTonCellError, TonCellError};

lazy_static! {
    pub static ref CRC_32_ISCSI: Crc<u32> = Crc::<u32>::new(&crc::CRC_32_ISCSI);
}

/// Raw representation of Cell.
///
/// References are stored as indices in BagOfCells.
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub(crate) struct RawCell {
    pub(crate) data: Vec<u8>,
    pub(crate) bit_len: usize,
    pub(crate) references: Vec<usize>,
    pub(crate) is_exotic: bool,
    level_mask: u32,
}

impl RawCell {
    pub(crate) fn new(
        data: Vec<u8>,
        bit_len: usize,
        references: Vec<usize>,
        level_mask: u32,
        is_exotic: bool,
    ) -> Self {
        Self {
            data,
            bit_len,
            references,
            level_mask: level_mask & 7,
            is_exotic,
        }
    }
}

/// Raw representation of BagOfCells.
///
/// `cells` must be topologically sorted.
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub(crate) struct RawBagOfCells {
    pub(crate) cells: Vec<RawCell>,
    pub(crate) roots: Vec<usize>,
}

const GENERIC_BOC_MAGIC: u32 = 0xb5ee9c72;
const _INDEXED_BOC_MAGIC: u32 = 0x68ff65f3;
const _INDEXED_CRC32_MAGIC: u32 = 0xacc3a728;

impl RawBagOfCells {
    pub(crate) fn parse(serial: &[u8]) -> Result<RawBagOfCells, TonCellError> {
        let cursor = Cursor::new(serial);

        let mut reader: ByteReader<Cursor<&[u8]>, BigEndian> =
            ByteReader::endian(cursor, BigEndian);
        // serialized_boc#b5ee9c72
        let magic = reader.read::<u32>().map_boc_deserialization_error()?;

        let (has_idx, has_crc32c, _has_cache_bits, size) = match magic {
            GENERIC_BOC_MAGIC => {
                // has_idx:(## 1) has_crc32c:(## 1) has_cache_bits:(## 1) flags:(## 2) { flags = 0 }
                let header = reader.read::<u8>().map_boc_deserialization_error()?;
                let has_idx = (header >> 7) & 1 == 1;
                let has_crc32c = (header >> 6) & 1 == 1;
                let has_cache_bits = (header >> 5) & 1 == 1;
                // size:(## 3) { size <= 4 }
                let size = header & 0b0000_0111;

                (has_idx, has_crc32c, has_cache_bits, size)
            }
            magic => {
                return Err(TonCellError::boc_deserialization_error(format!(
                    "Unsupported cell magic number: {:#}",
                    magic
                )));
            }
        };
        //   off_bytes:(## 8) { off_bytes <= 8 }
        let off_bytes = reader.read::<u8>().map_boc_deserialization_error()?;
        //cells:(##(size * 8))
        let cells = read_var_size(&mut reader, size)?;
        //   roots:(##(size * 8)) { roots >= 1 }
        let roots = read_var_size(&mut reader, size)?;
        //   absent:(##(size * 8)) { roots + absent <= cells }
        let _absent = read_var_size(&mut reader, size)?;
        //   tot_cells_size:(##(off_bytes * 8))
        let _tot_cells_size = read_var_size(&mut reader, off_bytes)?;
        //   root_list:(roots * ##(size * 8))
        let mut root_list = vec![];
        for _ in 0..roots {
            root_list.push(read_var_size(&mut reader, size)?)
        }
        //   index:has_idx?(cells * ##(off_bytes * 8))
        let mut index = vec![];
        if has_idx {
            for _ in 0..cells {
                index.push(read_var_size(&mut reader, off_bytes)?)
            }
        }
        //   cell_data:(tot_cells_size * [ uint8 ])
        let mut cell_vec = Vec::with_capacity(cells);
        for _ in 0..cells {
            let cell = read_cell(&mut reader, size)?;
            cell_vec.push(cell);
        }

        if has_crc32c {

            let position = reader.reader().position();
            let crc_bytes = &serial[position as usize..];

            let without_last_4 = &serial[..serial.len().saturating_sub(4)];
            let check_sum = CRC_32_ISCSI.checksum(without_last_4).to_le_bytes();
            println!("dfdsf");

            if crc_bytes != check_sum {
                return Err(TonCellError::boc_deserialization_error("Crc32 mismatch".to_string()));
            };
        };

        Ok(RawBagOfCells {
            cells: cell_vec,
            roots: root_list,
        })
    }

    pub(crate) fn serialize(&self, has_crc32: bool) -> Result<Vec<u8>, TonCellError> {
        //Based on https://github.com/toncenter/tonweb/blob/c2d5d0fc23d2aec55a0412940ce6e580344a288c/src/boc/Cell.js#L198

        let root_count = self.roots.len();
        let num_ref_bits = 32 - (self.cells.len() as u32).leading_zeros();
        let num_ref_bytes = (num_ref_bits + 7) / 8;
        let has_idx = false;

        let mut full_size = 0u32;

        for cell in &self.cells {
            full_size += raw_cell_size(cell, num_ref_bytes);
        }

        let num_offset_bits = 32 - full_size.leading_zeros();
        let num_offset_bytes = (num_offset_bits + 7) / 8;

        let total_size = 4 + // magic
            1 + // flags and s_bytes
            1 + // offset_bytes
            3 * num_ref_bytes + // cells_num, roots, complete
            num_offset_bytes + // full_size
            num_ref_bytes + // root_idx
            (if has_idx { self.cells.len() as u32 * num_offset_bytes } else { 0 }) +
            full_size +
            (if has_crc32 { 4 } else { 0 });

        let mut writer = BitWriter::endian(Vec::with_capacity(total_size as usize), BigEndian);

        writer
            .write(32, GENERIC_BOC_MAGIC)
            .map_boc_serialization_error()?;

        //write flags byte
        let has_cache_bits = false;
        let flags: u8 = 0;
        writer.write_bit(has_idx).map_boc_serialization_error()?;
        writer.write_bit(has_crc32).map_boc_serialization_error()?;
        writer
            .write_bit(has_cache_bits)
            .map_boc_serialization_error()?;
        writer.write(2, flags).map_boc_serialization_error()?;
        writer
            .write(3, num_ref_bytes)
            .map_boc_serialization_error()?;
        writer
            .write(8, num_offset_bytes)
            .map_boc_serialization_error()?;
        writer
            .write(8 * num_ref_bytes, self.cells.len() as u32)
            .map_boc_serialization_error()?;
        writer
            .write(8 * num_ref_bytes, root_count as u32)
            .map_boc_serialization_error()?;
        writer
            .write(8 * num_ref_bytes, 0)
            .map_boc_serialization_error()?; // Complete BOCs only
        writer
            .write(8 * num_offset_bytes, full_size)
            .map_boc_serialization_error()?;
        for &root in &self.roots {
            writer
                .write(8 * num_ref_bytes, root as u32)
                .map_boc_serialization_error()?;
        }

        for cell in &self.cells {
            write_raw_cell(&mut writer, cell, num_ref_bytes)?;
        }

        if has_crc32 {
            let bytes = writer.writer().ok_or_else(|| {
                TonCellError::boc_serialization_error("Stream is not byte-aligned")
            })?;
            let cs = CRC_32_ISCSI.checksum(bytes.as_slice());
            writer
                .write_bytes(cs.to_le_bytes().as_slice())
                .map_boc_serialization_error()?;
        }
        writer.byte_align().map_boc_serialization_error()?;
        let res = writer
            .writer()
            .ok_or_else(|| TonCellError::boc_serialization_error("Stream is not byte-aligned"))?;
        Ok(res.clone())
    }
}

fn read_cell(
    reader: &mut ByteReader<Cursor<&[u8]>, BigEndian>,
    size: u8,
) -> Result<RawCell, TonCellError> {
    let d1 = reader.read::<u8>().map_boc_deserialization_error()?;
    let d2 = reader.read::<u8>().map_boc_deserialization_error()?;

    let ref_num = d1 & 0b111;
    let is_exotic = (d1 & 0b1000) != 0;
    let has_hashes = (d1 & 0b10000) != 0;
    let level_mask = (d1 >> 5) as u32;
    let data_size = ((d2 >> 1) + (d2 & 1)).into();
    let full_bytes = (d2 & 0x01) == 0;

    if has_hashes {
        let hash_count = LevelMask::new(level_mask).hash_count();
        let skip_size = hash_count * (32 + 2);

        // TODO: check depth and hashes
        reader
            .skip(skip_size as u32)
            .map_boc_deserialization_error()?;
    }

    let mut data = reader
        .read_to_vec(data_size)
        .map_boc_deserialization_error()?;

    let data_len = data.len();
    let padding_len = if data_len > 0 && !full_bytes {
        // Fix last byte,
        // see https://github.com/toncenter/tonweb/blob/c2d5d0fc23d2aec55a0412940ce6e580344a288c/src/boc/BitString.js#L302
        let num_zeros = data[data_len - 1].trailing_zeros();
        if num_zeros >= 8 {
            return Err(TonCellError::boc_deserialization_error(
                "Last byte of binary must not be zero if full_byte flag is not set",
            ));
        }
        data[data_len - 1] &= !(1 << num_zeros);
        num_zeros + 1
    } else {
        0
    };
    let bit_len = data.len() * 8 - padding_len as usize;
    let mut references: Vec<usize> = Vec::new();
    for _ in 0..ref_num {
        references.push(read_var_size(reader, size)?);
    }
    let cell = RawCell::new(data, bit_len, references, level_mask, is_exotic);
    Ok(cell)
}

fn raw_cell_size(cell: &RawCell, ref_size_bytes: u32) -> u32 {
    let data_len = (cell.bit_len + 7) / 8;
    2 + data_len as u32 + cell.references.len() as u32 * ref_size_bytes
}

fn write_raw_cell(
    writer: &mut BitWriter<Vec<u8>, BigEndian>,
    cell: &RawCell,
    ref_size_bytes: u32,
) -> Result<(), TonCellError> {
    let level = cell.level_mask;
    let is_exotic = cell.is_exotic as u32;
    let num_refs = cell.references.len() as u32;
    let d1 = num_refs + is_exotic * 8 + level * 32;

    let padding_bits = cell.bit_len % 8;
    let full_bytes = padding_bits == 0;
    let data = cell.data.as_slice();
    let data_len_bytes = (cell.bit_len + 7) / 8;
    // data_len_bytes <= 128 by spec, but d2 must be u8 by spec as well
    let d2 = (data_len_bytes * 2 - if full_bytes { 0 } else { 1 }) as u8; //subtract 1 if the last byte is not full

    writer.write(8, d1).map_boc_serialization_error()?;
    writer.write(8, d2).map_boc_serialization_error()?;
    if !full_bytes {
        writer
            .write_bytes(&data[..data_len_bytes - 1])
            .map_boc_serialization_error()?;
        let last_byte = data[data_len_bytes - 1];
        let l = last_byte | 1 << (8 - padding_bits - 1);
        writer.write(8, l).map_boc_serialization_error()?;
    } else {
        writer.write_bytes(data).map_boc_serialization_error()?;
    }

    for r in cell.references.as_slice() {
        writer
            .write(8 * ref_size_bytes, *r as u32)
            .map_boc_serialization_error()?;
    }

    Ok(())
}

fn read_var_size(
    reader: &mut ByteReader<Cursor<&[u8]>, BigEndian>,
    n: u8,
) -> Result<usize, TonCellError> {
    let bytes = reader
        .read_to_vec(n.into())
        .map_boc_deserialization_error()?;

    let mut result = 0;
    for &byte in &bytes {
        result <<= 8;
        result |= usize::from(byte);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use hex::FromHex;
    use tokio_test::assert_ok;
    use super::*;

    #[test]
    fn test_raw_cell_serialize() {
        let raw_cell = RawCell::new(vec![1; 128], 1023, vec![], 255, false);
        let raw_bag = RawBagOfCells {
            cells: vec![raw_cell],
            roots: vec![0],
        };
        assert!(raw_bag.serialize(false).is_ok());
    }

    #[test]
    fn test_crc32() -> anyhow::Result<()> {
        let a = "b5ee9c7241022101000739000114ff00f4a413f4bcf2c80b01020162050202012004030009bdb05c1ffc0007bfe45d440202c912060103b0f00704f62082300de0b6b3a7640000ba9b30823025b946ebc0b36173e08200c354218235c702bd3a30fc0000be228238070c1cc73b00c80000bbb0f2f420c1008e1282300de0b6b3a76400005202a3f04712a984e020821b782dace9d9aa18bee30f01a7648238056bc75e2d6310000021822056bc75e2d631aa18bee3002111100f0802f4822056bc75e2d631aa17be8e2701822056bc75e2d631aa17a101824adf0ab5a80a22c61ab5a7008238056bc75e2d63100000a984de21822056bc75e2d631aa16be8e2601822056bc75e2d631aa16a10182403f1fce3da636ea5cf8508238056bc75e2d63100000a984de21823815af1d78b58c400000bee300210e0902f482380ad78ebc5ac6200000be8e260182380ad78ebc5ac6200000a1018238280e60114edb805d038238056bc75e2d63100000a984de218238056bc75e2d63100000be8e26018238056bc75e2d63100000a10182380ebc5fb417461211108238056bc75e2d63100000a984de218232b5e3af16b1880000bee300210d0a01ec82315af1d78b58c40000be8e250182315af1d78b58c40000a101823806f5f17757889379378238056bc75e2d63100000a984de218238056bc75e2d6310000021a0511382380ad78ebc5ac6200000a98466a0511382381043561a8829300000a98466a05113823815af1d78b58c400000a98466a051130b01ea82381b1ae4d6e2ef500000a98466a0511382382086ac351052600000a98466a05113823825f273933db5700000a98466a05113822056bc75e2d631aa16a98466a05113823830ca024f987b900000a98466a0511382383635c9adc5dea00000a98466a0511382383ba1910bf341b00000a98466a0030c00428238410d586a20a4c00000a98412a08238056bc75e2d63100000a984018064a984004a018232b5e3af16b1880000a101823808f00f760a4b2db55d8238056bc75e2d63100000a984004c01823815af1d78b58c400000a101823927fa27722cc06cc5e28238056bc75e2d63100000a984003830822056bc75e2d631aa18a18261855144814a7ff805980ff0084000005020821b782dace9d9aa17be8e18821b782dace9d9aa17a182501425982cf597cd205cef73809171e20042821b782dace9d9aa18a18288195e54c5dd42177f53a27172fa9ec630262827aa230201201e130103aee01401f62082300de0b6b3a7640000b98e1182300de0b6b3a76400005202a984f03ba3e0702182b05803bcc5cb9634ba4cfb2213f784019318ed4dcb6017880faa35be8e23308288195e54c5dd42177f53a27172fa9ec630262827aa23a904821b782dace9d9aa18de2182708bcc0026baae9e45e470190267a230cfaa18be1502ea8e1c0182501425982cf597cd205cef7380a90401821b782dace9d9aa17a0dea76401a764208261855144814a7ff805980ff0084000be8e2a8238056bc75e2d631000008261855144814a7ff805980ff0084000a98401822056bc75e2d631aa18a001de20824adf0ab5a80a22c61ab5a700bee300201d1602f882403f1fce3da636ea5cf850be8e268238056bc75e2d6310000082403f1fce3da636ea5cf850a98401822056bc75e2d631aa16a001de20823927fa27722cc06cc5e2be8e268238056bc75e2d63100000823927fa27722cc06cc5e2a98401823815af1d78b58c400000a001de208238280e60114edb805d03bee300201c1702f482380ebc5fb41746121110be8e268238056bc75e2d6310000082380ebc5fb41746121110a984018238056bc75e2d63100000a001de20823808f00f760a4b2db55dbe8e258238056bc75e2d63100000823808f00f760a4b2db55da984018232b5e3af16b1880000a001de20823806f5f1775788937937bee300201b1801ec823806248f33704b286603be8e258238056bc75e2d63100000823806248f33704b286603a984018230ad78ebc5ac620000a001de20823805c548670b9510e7acbe8e258238056bc75e2d63100000823805c548670b9510e7aca98401823056bc75e2d6310000a001de208238056bc75e2d63100000a11901fe8238056bc75e2d631000005122a012a98453008238056bc75e2d63100000a9845c8238056bc75e2d63100000a9842073a90413a051218238056bc75e2d63100000a9842075a90413a051218238056bc75e2d63100000a9842077a90413a051218238056bc75e2d63100000a9842079a90413a0598238056bc75e2d631000001a001ca984800ba904a0aa00a08064a904004a8238056bc75e2d63100000823806f5f1775788937937a9840182315af1d78b58c40000a001004c8238056bc75e2d631000008238280e60114edb805d03a9840182380ad78ebc5ac6200000a001004e8238056bc75e2d63100000824adf0ab5a80a22c61ab5a700a98401822056bc75e2d631aa17a001020120201f0063a46410e0804c45896c678b00d180ef381038c70a023d5486531812d40950025503815210e0002298731819d5016780e4e8400005d17c12\
        6e3e0998"; /* crc32 part*/
        let serial_vec = Vec::from_hex(&a)?;
        let serial: [u8; 1024] = serial_vec.try_into().unwrap();
        let result = RawBagOfCells::parse(&serial);
        assert_ok!(result);
        Ok(())
    }
}
