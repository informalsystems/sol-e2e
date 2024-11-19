pub mod network;
pub mod scenario;

use network::anvil::AnvilPoA;
use network::ethpkg::EthPkgKurtosis;
use network::EthereumNetwork as Network;
use scenario::Scenario;
use testresult::TestResult;

use rstest::rstest;

#[rstest]
#[case(AnvilPoA::default(), scenario::erc20::ERC20Transfer)]
#[case(EthPkgKurtosis::default(), scenario::erc20::ERC20Transfer)]
#[tokio::test]
async fn test_eth_e2e(
    #[case] mut network: impl Network,
    #[case] scenario: impl Scenario,
) -> TestResult {
    network.start().await?;
    while network.health_check().await.is_err() {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    let config = network.network_config();
    let resp = scenario.run(config).await;
    network.stop().await?;
    resp
}