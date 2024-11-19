use core::net::Ipv4Addr;
use testresult::TestResult;

use alloy::node_bindings::{Anvil, AnvilInstance};

use crate::tests::network::{EthereumConfig, EthereumNetwork};
use bon::Builder;

#[derive(Builder, Debug)]
pub struct AnvilPoA {
    #[builder(default = 8545)]
    pub port: u16,
    #[builder(default = 1)]
    pub block_time: u64,
    #[builder(
        default = "abstract vacuum mammal awkward pudding scene penalty purchase dinner depart evoke puzzle".into()
    )]
    pub mnemonic: String,
    pub process: Option<AnvilInstance>,
}

impl Default for AnvilPoA {
    fn default() -> Self {
        Self::builder().build()
    }
}

impl EthereumNetwork for AnvilPoA {
    async fn start(&mut self) -> TestResult {
        if self.process.is_some() {
            panic!();
        }

        self.process = Some(
            Anvil::new()
                .port(self.port)
                .block_time(self.block_time)
                .mnemonic(self.mnemonic.clone())
                .spawn(),
        );

        Ok(())
    }

    fn network_config(&self) -> EthereumConfig {
        EthereumConfig {
            ip: Ipv4Addr::UNSPECIFIED.into(),
            port: self.port,
            mnemonics: vec![self.mnemonic.clone()],
            block_time: self.block_time,
        }
    }

    async fn stop(self) -> TestResult {
        drop(self.process);
        Ok(())
    }
}
