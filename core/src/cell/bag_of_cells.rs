use std::sync::Arc;

use base64::engine::general_purpose::STANDARD;

use crate::cell::raw_boc_from_boc::convert_to_raw_boc;
use crate::cell::*;

#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct BagOfCells {
    pub roots: Vec<ArcCell>,
}

impl BagOfCells {
    pub fn new(roots: &[ArcCell]) -> BagOfCells {
        BagOfCells {
            roots: roots.to_vec(),
        }
    }

    pub fn from_root(root: Cell) -> BagOfCells {
        let arc = Arc::new(root);
        BagOfCells { roots: vec![arc] }
    }

    pub fn add_root(&mut self, root: Cell) {
        let arc = Arc::new(root);
        self.roots.push(arc)
    }

    pub fn num_roots(&self) -> usize {
        self.roots.len()
    }

    pub fn root(&self, idx: usize) -> Result<&ArcCell, TonCellError> {
        self.roots.get(idx).ok_or_else(|| {
            TonCellError::boc_deserialization_error(format!(
                "Invalid root index: {}, BoC contains {} roots",
                idx,
                self.roots.len()
            ))
        })
    }

    pub fn single_root(&self) -> Result<&ArcCell, TonCellError> {
        let root_count = self.roots.len();
        if root_count == 1 {
            Ok(&self.roots[0])
        } else {
            Err(TonCellError::CellParserError(format!(
                "Single root expected, got {}",
                root_count
            )))
        }
    }

    pub fn into_single_root(&mut self) -> Result<ArcCell, TonCellError> {
        let root_count = self.roots.len();
        if root_count == 1 {
            Ok(self.roots.pop().unwrap()) // unwrap is safe: we have checked that roots has exactly one element above
        } else {
            Err(TonCellError::CellParserError(format!(
                "Single root expected, got {}",
                root_count
            )))
        }
    }

    pub fn parse(serial: &[u8]) -> Result<BagOfCells, TonCellError> {
        let raw = RawBagOfCells::parse(serial)?;
        let num_cells = raw.cells.len();
        let mut cells: Vec<ArcCell> = Vec::with_capacity(num_cells);

        for (cell_index, raw_cell) in raw.cells.into_iter().enumerate().rev() {
            let mut references = Vec::with_capacity(raw_cell.references.len());
            for ref_index in &raw_cell.references {
                if *ref_index <= cell_index {
                    return Err(TonCellError::boc_deserialization_error(
                        "References to previous cells are not supported",
                    ));
                }
                references.push(cells[num_cells - 1 - ref_index].clone());
            }

            let cell = Cell::new(
                raw_cell.data,
                raw_cell.bit_len,
                references,
                raw_cell.is_exotic,
            )
            .map_boc_deserialization_error()?;
            cells.push(cell.to_arc());
        }

        let roots = raw
            .roots
            .into_iter()
            .map(|r| &cells[num_cells - 1 - r])
            .map(Arc::clone)
            .collect();

        Ok(BagOfCells { roots })
    }

    pub fn parse_hex(hex: &str) -> Result<BagOfCells, TonCellError> {
        let str: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
        let bin = hex::decode(str.as_str()).map_boc_deserialization_error()?;
        Self::parse(&bin)
    }

    pub fn parse_base64(base64: &str) -> Result<BagOfCells, TonCellError> {
        let bin = STANDARD.decode(base64).map_boc_deserialization_error()?;
        Self::parse(&bin)
    }

    pub fn serialize(&self, has_crc32: bool) -> Result<Vec<u8>, TonCellError> {
        let raw = convert_to_raw_boc(self)?;
        raw.serialize(has_crc32)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Instant;

    use tokio_test::assert_ok;

    use crate::cell::raw_boc_from_boc::convert_to_raw_boc;
    use crate::cell::{BagOfCells, CellBuilder, TonCellError};
    use crate::message::ZERO_COINS;

    #[test]
    fn cell_hash_works() -> Result<(), TonCellError> {
        let hole_address = "EQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAM9c"
            .parse()
            .unwrap();
        let contract = "EQDwHr48oKCFD5od9u_TnsCOhe7tGZIei-5ESWfzhlWLRYvW"
            .parse()
            .unwrap();
        let token0 = "EQDk2VTvn04SUKJrW7rXahzdF8_Qi6utb0wj43InCu9vdjrR"
            .parse()
            .unwrap();
        let token1 = "EQAIcb1WqNr0E7rOXgO0cbAZQnVbS06mgH2vgBvtBE6p0T2a"
            .parse()
            .unwrap();
        let raw =
            "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\
        1MALHGAGMjLBVADzxZw+gISy2rMyYMG+wBA0lqA";

        let boc = BagOfCells::parse_base64(raw)?;
        let cell = boc.single_root()?;

        let mut boc = BagOfCells::parse_base64(raw)?;
        let intocell = boc.into_single_root()?;

        assert_eq!(cell, &intocell);

        let jetton_wallet_code_lp = cell.reference(0)?;
        let pool_code = cell.reference(1)?;
        let account_lp_code = cell.reference(2)?;

        let protocol_fee = CellBuilder::new()
            .store_coins(&ZERO_COINS)?
            .store_coins(&ZERO_COINS)?
            .store_raw_address(&hole_address)?
            .store_coins(&ZERO_COINS)?
            .store_coins(&ZERO_COINS)?
            .build()?;

        let data = CellBuilder::new()
            .store_address(&contract)?
            .store_u8(4, 2)?
            .store_u8(4, 0)?
            .store_u8(4, 1)?
            .store_address(&token0)?
            .store_address(&token1)?
            .store_coins(&ZERO_COINS)?
            .store_reference(&Arc::new(protocol_fee))?
            .store_reference(jetton_wallet_code_lp)?
            .store_reference(account_lp_code)?
            .build()?;

        let state = CellBuilder::new()
            .store_bit(false)? //Split depth
            .store_bit(false)? //Ticktock
            .store_bit(true)? //Code
            .store_bit(true)? //Data
            .store_bit(false)? //Library
            .store_reference(pool_code)?
            .store_reference(&Arc::new(data))?
            .build()?;

        let hash = state.cell_hash().to_hex();
        assert_eq!(
            hash,
            "e557059d5395a79f714ddb966e8419d4681f0ce4aa966cf6088db610841c204a"
        );

        Ok(())
    }

    #[ignore]
    #[test]
    fn check_code_hash() -> Result<(), TonCellError> {
        let raw = include_str!("../../resources/wallet/wallet_v3r1.code");
        let boc = BagOfCells::parse_base64(raw)?;
        println!(
            "wallet_v3_code code_hash{:?}",
            boc.single_root()?.cell_hash_base64()
        );

        let raw = include_str!("../../resources/wallet/wallet_v3r2.code");
        let boc = BagOfCells::parse_base64(raw)?;
        println!(
            "wallet_v3r2_code code_hash{:?}",
            boc.single_root()?.cell_hash_base64()
        );

        let raw = include_str!("../../resources/wallet/wallet_v4r2.code");
        let boc = BagOfCells::parse_base64(raw)?;
        println!(
            "wallet_v4r2_code code_hash{:?}",
            boc.single_root()?.cell_hash_base64()
        );
        Ok(())
    }

    #[ignore]
    #[test]
    fn benchmark_cell_repr() -> Result<(), TonCellError> {
        let now = Instant::now();
        for _ in 1..10000 {
            let result = cell_hash_works();
            match result {
                Ok(_) => {}
                Err(e) => return Err(e),
            }
        }
        let elapsed = now.elapsed();
        println!("Elapsed: {:.2?}", elapsed);
        Ok(())
        // initially it works for 10.39seceonds
    }

    #[test]
    fn it_constructs_raw() -> Result<(), TonCellError> {
        let leaf = CellBuilder::new().store_byte(10)?.build()?;
        let inter = CellBuilder::new()
            .store_byte(20)?
            .store_child(leaf)?
            .build()?;
        let root = CellBuilder::new()
            .store_byte(30)?
            .store_child(inter)?
            .build()?;
        let boc = BagOfCells::from_root(root);
        let _raw = convert_to_raw_boc(&boc)?;
        Ok(())
    }

    #[test]
    fn test_crc32() -> Result<(), TonCellError> {
        // b5ee9c72 - magic number
        // 41010201 - metadata
        // 68656c6c6f -"Hello" in ASCII + metadata
        // 544f4e - "TON" in ASCII
        // 626c6f636b636861696e - "Blockchain" in ASCII.
        // 2fc7a7b3 - crc32
        // let a = "b5ee9c72\
        //     41010201\
        //     000200000101000f480068656c6c6f\
        //     008c2c0000544f4e\
        //     008c2c0000626c6f636b636861696e\
        //     2fc7a7b3";

        let a = "b5ee9c7241022101000739000114ff00f4a413f4bcf2c80b01020162050202012004030009bdb05c1ffc0007bfe45d440202c912060103b0f00704f62082300de0b6b3a7640000ba9b30823025b946ebc0b36173e08200c354218235c702bd3a30fc0000be228238070c1cc73b00c80000bbb0f2f420c1008e1282300de0b6b3a76400005202a3f04712a984e020821b782dace9d9aa18bee30f01a7648238056bc75e2d6310000021822056bc75e2d631aa18bee3002111100f0802f4822056bc75e2d631aa17be8e2701822056bc75e2d631aa17a101824adf0ab5a80a22c61ab5a7008238056bc75e2d63100000a984de21822056bc75e2d631aa16be8e2601822056bc75e2d631aa16a10182403f1fce3da636ea5cf8508238056bc75e2d63100000a984de21823815af1d78b58c400000bee300210e0902f482380ad78ebc5ac6200000be8e260182380ad78ebc5ac6200000a1018238280e60114edb805d038238056bc75e2d63100000a984de218238056bc75e2d63100000be8e26018238056bc75e2d63100000a10182380ebc5fb417461211108238056bc75e2d63100000a984de218232b5e3af16b1880000bee300210d0a01ec82315af1d78b58c40000be8e250182315af1d78b58c40000a101823806f5f17757889379378238056bc75e2d63100000a984de218238056bc75e2d6310000021a0511382380ad78ebc5ac6200000a98466a0511382381043561a8829300000a98466a05113823815af1d78b58c400000a98466a051130b01ea82381b1ae4d6e2ef500000a98466a0511382382086ac351052600000a98466a05113823825f273933db5700000a98466a05113822056bc75e2d631aa16a98466a05113823830ca024f987b900000a98466a0511382383635c9adc5dea00000a98466a0511382383ba1910bf341b00000a98466a0030c00428238410d586a20a4c00000a98412a08238056bc75e2d63100000a984018064a984004a018232b5e3af16b1880000a101823808f00f760a4b2db55d8238056bc75e2d63100000a984004c01823815af1d78b58c400000a101823927fa27722cc06cc5e28238056bc75e2d63100000a984003830822056bc75e2d631aa18a18261855144814a7ff805980ff0084000005020821b782dace9d9aa17be8e18821b782dace9d9aa17a182501425982cf597cd205cef73809171e20042821b782dace9d9aa18a18288195e54c5dd42177f53a27172fa9ec630262827aa230201201e130103aee01401f62082300de0b6b3a7640000b98e1182300de0b6b3a76400005202a984f03ba3e0702182b05803bcc5cb9634ba4cfb2213f784019318ed4dcb6017880faa35be8e23308288195e54c5dd42177f53a27172fa9ec630262827aa23a904821b782dace9d9aa18de2182708bcc0026baae9e45e470190267a230cfaa18be1502ea8e1c0182501425982cf597cd205cef7380a90401821b782dace9d9aa17a0dea76401a764208261855144814a7ff805980ff0084000be8e2a8238056bc75e2d631000008261855144814a7ff805980ff0084000a98401822056bc75e2d631aa18a001de20824adf0ab5a80a22c61ab5a700bee300201d1602f882403f1fce3da636ea5cf850be8e268238056bc75e2d6310000082403f1fce3da636ea5cf850a98401822056bc75e2d631aa16a001de20823927fa27722cc06cc5e2be8e268238056bc75e2d63100000823927fa27722cc06cc5e2a98401823815af1d78b58c400000a001de208238280e60114edb805d03bee300201c1702f482380ebc5fb41746121110be8e268238056bc75e2d6310000082380ebc5fb41746121110a984018238056bc75e2d63100000a001de20823808f00f760a4b2db55dbe8e258238056bc75e2d63100000823808f00f760a4b2db55da984018232b5e3af16b1880000a001de20823806f5f1775788937937bee300201b1801ec823806248f33704b286603be8e258238056bc75e2d63100000823806248f33704b286603a984018230ad78ebc5ac620000a001de20823805c548670b9510e7acbe8e258238056bc75e2d63100000823805c548670b9510e7aca98401823056bc75e2d6310000a001de208238056bc75e2d63100000a11901fe8238056bc75e2d631000005122a012a98453008238056bc75e2d63100000a9845c8238056bc75e2d63100000a9842073a90413a051218238056bc75e2d63100000a9842075a90413a051218238056bc75e2d63100000a9842077a90413a051218238056bc75e2d63100000a9842079a90413a0598238056bc75e2d631000001a001ca984800ba904a0aa00a08064a904004a8238056bc75e2d63100000823806f5f1775788937937a9840182315af1d78b58c40000a001004c8238056bc75e2d631000008238280e60114edb805d03a9840182380ad78ebc5ac6200000a001004e8238056bc75e2d63100000824adf0ab5a80a22c61ab5a700a98401822056bc75e2d631aa17a001020120201f0063a46410e0804c45896c678b00d180ef381038c70a023d5486531812d40950025503815210e0002298731819d5016780e4e8400005d17c12\
        6e3e099811"; /* crc32 part*/
        // let a = "b5ee9c7201010101002800004c0168747470733a2f2f6e66742e667261676d656e742e636f6d2f6e756d626572732e6a736f6e";
        println!("a: {:?}", a);
        assert_ok!(BagOfCells::parse_hex(a));
        Ok(())
    }
}
