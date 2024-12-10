use num_bigint::BigUint;
use sha2::{Digest, Sha256};
use std::str::FromStr;
use tokio_test::assert_ok;
use tonlib_client::contract::{
    NftCollectionContract, NftItemContract, NftItemData, TonContractFactory,
};
use tonlib_client::meta::MetaDataContent::External;
use tonlib_client::meta::{LoadMeta, MetaDataContent, NftColletionMetaLoader, NftItemMetaLoader};
use tonlib_core::{TonAddress, TonHash};

mod common;

// ---- A group of tests that the methods basically work
#[tokio::test]
async fn test_get_nft_data() {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = assert_ok!(TonContractFactory::builder(&client).build().await);
    let contract = factory.get_contract(&assert_ok!(
        "EQBKwtMZSZurMxGp7FLZ_lM9t54_ECEsS46NLR3qfIwwTnKW".parse()
    ));
    assert_ok!(contract.get_nft_data().await);
}

#[tokio::test]
async fn test_get_nft_collection_data() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract =
        factory.get_contract(&"EQB2iHQ9lmJ9zvYPauxN9hVOfHL3c_fuN5AyRq5Pm84UH6jC".parse()?);
    assert_ok!(contract.get_collection_data().await);
    Ok(())
}

#[tokio::test]
async fn test_get_nft_address_by_index() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract = factory.get_contract(&assert_ok!(
        "EQB2iHQ9lmJ9zvYPauxN9hVOfHL3c_fuN5AyRq5Pm84UH6jC".parse()
    ));
    assert_ok!(contract.get_nft_address_by_index(2).await);
    Ok(())
}

// ---- A group of tests that methods return valid data
#[tokio::test]
async fn test_get_nft_data_is_valid() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract = factory.get_contract(&assert_ok!(
        "EQCGZEZZcYO9DK877fJSIEpYMSvfui7zmTXGhq0yq1Ce1Mb6".parse()
    ));
    let res = assert_ok!(contract.get_nft_data().await);
    log::info!("{:#?}", res);

    let expected_collection_address = assert_ok!(TonAddress::from_base64_url(
        &"EQAOQdwdw8kGftJCSFgOErM1mBjYPe4DBPq8-AhF6vr9si5N".to_string()
    ));
    let expected_owner_address = assert_ok!(TonAddress::from_base64_url(
        &"EQCgjHh831e9_qlCWLgaAwEIQ8qOolUT831vJF0bau6LMV5G".to_string()
    ));
    let expected_index = assert_ok!(BigUint::from_str(
        "15995005474673311991943775795727481451058346239240361725119718297821926435889",
    ));
    println!("{:?}", expected_collection_address);
    let expected_response = NftItemData {
        init: true,
        index: expected_index,
        collection_address: expected_collection_address,
        owner_address: expected_owner_address,
        individual_content: External {
            uri: "https://nft.fragment.com/number/88805397120.json".to_string(),
        },
    };
    assert_eq!(res, expected_response);
    Ok(())
}

#[tokio::test]
async fn test_get_nft_content_uri() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract = factory.get_contract(&assert_ok!(
        "EQCGZEZZcYO9DK877fJSIEpYMSvfui7zmTXGhq0yq1Ce1Mb6".parse()
    ));
    let res = assert_ok!(contract.get_nft_data().await);

    let expected_uri = "https://nft.fragment.com/number/88805397120.json".to_string();
    assert_eq!(
        res.individual_content,
        MetaDataContent::External { uri: expected_uri }
    );
    Ok(())
}

#[tokio::test]
async fn test_get_nft_content_arkenston() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = assert_ok!(TonContractFactory::builder(&client).build().await);
    let contract = factory.get_contract(&assert_ok!(
        "EQDhR36C8pSVtyhOFtE9nh2DFq4WYUbTZFmvjfnShlrXq2cz".parse()
    ));
    let res = assert_ok!(contract.get_nft_data().await);

    // Пилить ниже
    let meta_loader = assert_ok!(NftItemMetaLoader::default());
    let content_res = assert_ok!(meta_loader.load(&res.individual_content).await);
    assert_eq!(
        content_res.image.unwrap(),
        "https://static.ston.fi/stake-nft/i4.jpg"
    );
    assert_eq!(content_res.name.unwrap(), "ARKENSTON NFT");
    Ok(())
}

#[tokio::test]
async fn test_get_nft_content_some() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract = factory.get_contract(&assert_ok!(
        "EQCiXgoveScGKGGqo50HbmwP3goKJaEfu9QmeBRJ-jbRxM21".parse()
    ));
    let res = assert_ok!(contract.get_nft_data().await);

    // Пилить здесь.
    let meta_loader = assert_ok!(NftItemMetaLoader::default());
    let content_res = assert_ok!(meta_loader.load(&res.individual_content).await);
    assert_eq!(
        content_res.image.unwrap(),
        "https://mars.tonplanets.com/i/biomes/4v4.jpg"
    );
    assert_eq!(content_res.name.unwrap(), "Anda");
    Ok(())
}

// ---------------------nft get collection metadata tests

#[tokio::test]
async fn test_get_nft_collection_content_uri() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_archive_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract = factory.get_contract(&assert_ok!(
        "EQAOQdwdw8kGftJCSFgOErM1mBjYPe4DBPq8-AhF6vr9si5N".parse()
    ));
    let res = assert_ok!(contract.get_collection_data().await);

    assert_eq!(
        res.collection_content,
        MetaDataContent::External {
            uri: "https://nft.fragment.com/numbers.json".to_string()
        }
    );

    let meta_loader = NftColletionMetaLoader::default()?;
    let content_res = assert_ok!(meta_loader.load(&res.collection_content).await);
    assert_eq!(
        content_res.name.as_ref().unwrap(),
        &String::from("Anonymous Telegram Numbers")
    );
    assert_eq!(
        content_res.image.as_ref().unwrap(),
        &String::from("https://nft.fragment.com/numbers.svg")
    );
    Ok(())
}

#[tokio::test]
async fn test_get_nft_collection_content_arkenston() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = assert_ok!(TonContractFactory::builder(&client).build().await);
    let contract = factory.get_contract(&assert_ok!(
        "EQCshJXbbcn7cvSkaM0Z8NyI-2pNCJC5RTGZB-cRF-Pax1lY".parse()
    ));
    let res = assert_ok!(contract.get_collection_data().await);
    let meta_loader = assert_ok!(NftColletionMetaLoader::default());
    let content_res = assert_ok!(meta_loader.load(&res.collection_content).await);
    assert_eq!(content_res.name.unwrap(), "ARKENSTON NFT");
    assert_eq!(
        content_res.image.unwrap(),
        "https://static.ston.fi/stake-nft/i1.jpg"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_nft_collection_content_some() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = assert_ok!(TonContractFactory::builder(&client).build().await);
    let contract = factory.get_contract(&assert_ok!(
        "EQCbOjwru5tBb2aaXZEHbiTCVIYQ6yDNAe8SSZkP4CozibHM".parse()
    ));
    let res = assert_ok!(contract.get_nft_data().await);
    let meta_loader = assert_ok!(NftColletionMetaLoader::default());
    let content_res = assert_ok!(meta_loader.load(&res.individual_content).await);
    assert_eq!(content_res.name.unwrap(), "Pokemon Pikachu #013 💎");
    assert_eq!(
        content_res.image.unwrap(),
        "https://s.getgems.io/nft/c/64284ddbde940b5d6ebc34f8/12/image.png"
    );
    Ok(())
}

#[tokio::test]
async fn test_get_nft_content_external() -> anyhow::Result<()> {
    common::init_logging();
    let client = common::new_mainnet_client().await;
    let factory = TonContractFactory::builder(&client).build().await?;
    let contract =
        factory.get_contract(&"EQDUF9cLVBH3BgziwOAIkezUdmfsDxxJHd6WSv0ChIUXYwCx".parse()?);
    let nft_data = contract.get_nft_data().await?;
    let internal = match nft_data.individual_content {
        MetaDataContent::Internal { dict } => dict,
        _ => panic!("Expected internal content"),
    };

    let expected_key = {
        let mut hasher: Sha256 = Sha256::new();
        hasher.update("public_keys");
        let slice = &hasher.finalize()[..];
        TryInto::<TonHash>::try_into(slice)?
    };
    assert!(internal.contains_key(&expected_key));
    Ok(())
}
