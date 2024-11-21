use core::future::Future;

use testresult::TestResult;

use crate::tests::network::EthereumConfig;

pub mod erc20;
pub mod finality;

pub trait Scenario {
    fn run(&self, config: EthereumConfig) -> impl Future<Output = TestResult> + Send;
}
