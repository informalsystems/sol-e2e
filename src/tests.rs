use alloy::network::Ethereum;
use alloy::network::EthereumWallet;
use alloy::network::NetworkWallet;
use alloy::node_bindings::Anvil;
use alloy::primitives::{Address, FixedBytes, U256};
use alloy::providers::Provider;
use alloy::providers::ProviderBuilder;
use alloy::providers::WalletProvider;
use alloy_signer_local::coins_bip39::English;
use alloy_signer_local::MnemonicBuilder;
use alloy_sol_types::sol;
use anyhow::Context;
use testresult::TestResult;

sol!(
    #[sol(rpc)]
    #[derive(Debug)]
    Erc20,
    "out/erc20.sol/Erc20.json",
);

#[tokio::test]
async fn test_deploy() -> TestResult<()> {
    let port = 8545u16;
    let url = format!("http://localhost:{}", port).to_string();

    let mnemonic =
        "abstract vacuum mammal awkward pudding scene penalty purchase dinner depart evoke puzzle";

    let block_time = 1;

    let anvil = Anvil::new()
        .port(port)
        .block_time(block_time)
        .mnemonic(mnemonic)
        .spawn();

    let wallet = MnemonicBuilder::<English>::default()
        .phrase(mnemonic)
        .build()?;

    let ethereum_wallet = EthereumWallet::new(wallet);

    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(ethereum_wallet)
        .on_builtin(&url)
        .await?;

    let name = "MyToken".to_string();
    let symbol = "MTK".to_string();
    let decimals = 18u8;
    let total_supply = U256::from(1_000_000);

    let contract = Erc20::deploy(
        &provider,
        name.clone(),
        symbol.clone(),
        decimals,
        total_supply,
    )
    .await?;

    let sender_address = NetworkWallet::<Ethereum>::default_signer_address(provider.wallet());

    tokio::time::sleep(std::time::Duration::from_secs(block_time)).await;

    let token_name = contract.name().call().await?;
    assert_eq!(token_name._0, name);

    let token_symbol = contract.symbol().call().await?;
    assert_eq!(token_symbol._0, symbol);

    let token_total_supply = contract.totalSupply().call().await?;
    assert_eq!(token_total_supply._0, total_supply);

    let sender_balance = contract.balanceOf(sender_address).call().await?;
    assert_eq!(sender_balance._0, total_supply);

    let recipient = Address::repeat_byte(1);
    let transfer_amount = U256::from(500);
    let transfer_call = contract.transfer(recipient, transfer_amount);

    let pending_tx = transfer_call.send().await?;
    let tx_hash: FixedBytes<32> = *pending_tx.tx_hash();

    tokio::time::sleep(std::time::Duration::from_secs(block_time)).await;

    let receipt = provider
        .get_transaction_receipt(tx_hash)
        .await?
        .context("No receipt")?;
    let _block_number = receipt.block_number.context("No block number")?;

    let recipient_balance = contract.balanceOf(recipient).call().await?;
    assert_eq!(recipient_balance._0, U256::from(500));

    let sender_balance = contract.balanceOf(sender_address).call().await?;
    assert_eq!(sender_balance._0, U256::from(999_500));

    // drop anvil
    drop(anvil);

    Ok(())
}
