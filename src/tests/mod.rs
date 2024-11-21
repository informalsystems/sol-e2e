pub mod network;
pub mod scenario;

use network::anvil::AnvilPoA;
use network::ethpkg::EthPkgKurtosis;
use network::EthereumNetwork as Network;
use scenario::Scenario;
use testresult::TestResult;

use rstest::rstest;

#[rstest]
#[case::anvil_erc20_transfer(AnvilPoA::default(), scenario::erc20::ERC20Transfer)]
#[case::kurtosis_erc20_transfer(EthPkgKurtosis::default(), scenario::erc20::ERC20Transfer)]
#[case::kurtosis_finality(EthPkgKurtosis::default(), scenario::finality::Finality)]
#[tokio::test]
async fn test_beacon_e2e(
    #[case] mut network: impl Network,
    #[case] scenario: impl Scenario,
) -> TestResult {
    network.start().await?;

    let result = {
        tokio::time::timeout(tokio::time::Duration::from_secs(180), async {
            loop {
                if network.health_check().await.is_ok() {
                    break;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        })
        .await?;

        let config = network.network_config();

        scenario.run(config).await
    };

    network.stop().await?;
    result
}
