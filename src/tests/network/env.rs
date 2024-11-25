use testresult::TestResult;

use crate::tests::network::{EthereumConfig, EthereumNetwork};

pub struct EnvNetwork;

impl EthereumNetwork for EnvNetwork {
    async fn start(&mut self) -> TestResult {
        Ok(())
    }

    fn network_config(&self) -> EthereumConfig {
        EthereumConfig {
            el_socket: std::env::var("EL_SOCKET")
                .expect("missing EL_SOCKET")
                .parse()
                .unwrap(),
            cl_socket: Some(
                std::env::var("CL_SOCKET")
                    .expect("missing CL_SOCKET")
                    .parse()
                    .unwrap(),
            ),
            mnemonics: vec![std::env::var("MNEMONIC")
                .expect("missing MNEMONIC")
                .to_string()],
            block_time: std::env::var("BLOCK_TIME")
                .expect("missing BLOCK_TIME")
                .parse()
                .unwrap(),
        }
    }

    async fn stop(self) -> TestResult {
        Ok(())
    }
}
