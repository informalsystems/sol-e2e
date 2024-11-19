use alloy::providers::{Provider, ProviderBuilder};
use core::future::Future;
use core::marker::Sync;
use core::net::IpAddr;
use testresult::TestResult;

pub mod anvil;
pub mod ethpkg;

pub struct EthereumConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub mnemonics: Vec<String>,
    pub block_time: u64,
}

pub trait EthereumNetwork: Sync + Send + Sized {
    fn start(&mut self) -> impl Future<Output = TestResult> + Send;
    fn network_config(&self) -> EthereumConfig;
    fn stop(self) -> impl Future<Output = TestResult> + Send;

    fn health_check(&self) -> impl Future<Output = TestResult> + Send {
        async {
            let EthereumConfig { ip, port, .. } = self.network_config();
            let provider = ProviderBuilder::new()
                .with_recommended_fillers()
                .on_builtin(&format!("http://{}:{}", ip, port))
                .await?;
            provider.get_chain_id().await?;
            Ok(())
        }
    }
}
