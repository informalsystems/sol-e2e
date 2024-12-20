use core::future::Future;
use core::marker::Sync;
use core::net::SocketAddr;

use alloy::providers::{Provider, ProviderBuilder};
use testresult::TestResult;

pub mod anvil;
pub mod env;
pub mod ethpkg;

pub struct EthereumConfig {
    pub el_socket: SocketAddr,
    pub cl_socket: Option<SocketAddr>,
    pub mnemonics: Vec<String>,
}

pub trait EthereumNetwork: Sync + Send + Sized {
    fn start(&mut self) -> impl Future<Output = TestResult> + Send;
    fn network_config(&self) -> EthereumConfig;
    fn stop(self) -> impl Future<Output = TestResult> + Send;

    fn health_check(&self) -> impl Future<Output = TestResult> + Send {
        async {
            let EthereumConfig { el_socket, .. } = self.network_config();
            let provider = ProviderBuilder::new()
                .with_recommended_fillers()
                .on_builtin(&format!("http://{}", el_socket))
                .await?;
            provider.get_chain_id().await?;
            Ok(())
        }
    }
}
