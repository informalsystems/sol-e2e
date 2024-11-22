use testresult::TestResult;

use super::{EthereumConfig, EthereumNetwork};

pub struct EnvNetwork;

impl EthereumNetwork for EnvNetwork {
    async fn start(&mut self) -> TestResult {
        Ok(())
    }

    fn network_config(&self) -> EthereumConfig {
        EthereumConfig {
            el_socket: env!("EL_SOCKET").parse().unwrap(),
            cl_socket: Some(env!("CL_SOCKET").parse().unwrap()),
            mnemonics: vec![env!("MNEMONIC").to_string()],
            block_time: env!("BLOCK_TIME").parse().unwrap(),
        }
    }

    async fn stop(self) -> TestResult {
        Ok(())
    }
}
