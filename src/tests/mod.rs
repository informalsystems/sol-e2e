pub mod network;
pub mod scenario;

use network::anvil::AnvilPoA;
use network::ethpkg::EthPkgKurtosis;
use network::EthereumNetwork as Network;
use scenario::erc20::ERC20Transfer;
use scenario::finality::{FinalityEndpoint, FinalityProtobuf};
use scenario::Scenario;
use testresult::TestResult;

use rstest::rstest;

#[rstest]
#[case::anvil_erc20_transfer(AnvilPoA::default(), ERC20Transfer)]
#[case::kurtosis_erc20_transfer(EthPkgKurtosis::default(), ERC20Transfer)]
#[case::kurtosis_finality_endpoint(EthPkgKurtosis::default(), FinalityEndpoint)]
#[case::kurtosis_finality_protobuf(EthPkgKurtosis::default(), FinalityProtobuf)]
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
