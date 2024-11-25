use core::net::{Ipv4Addr, SocketAddr};

use anyhow::Context;
use bon::Builder;
use kurtosis_sdk::enclave_api::api_container_service_client::ApiContainerServiceClient;
use kurtosis_sdk::enclave_api::starlark_run_response_line::RunResponseLine;
use kurtosis_sdk::enclave_api::{
    GetServicesArgs, ImageDownloadMode, RunStarlarkPackageArgs, ServiceInfo,
};
use kurtosis_sdk::engine_api::engine_service_client::EngineServiceClient;
use kurtosis_sdk::engine_api::{CreateEnclaveArgs, DestroyEnclaveArgs};
use serde_json::json;
use testresult::TestResult;

use crate::tests::network::{EthereumConfig, EthereumNetwork};

#[derive(Builder, Debug)]
pub struct EthPkgKurtosis {
    #[builder(default = SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 9710))]
    pub kurtosis_engine_endpoint: SocketAddr,
    #[builder(default = "ethpkg".into())]
    pub enclave_name: String,
    pub el_socket: Option<SocketAddr>,
    pub cl_socket: Option<SocketAddr>,
    #[builder(default = 1)]
    pub block_time: u64,
    #[builder(default = "abstract vacuum mammal awkward pudding scene penalty purchase dinner depart evoke puzzle".into())]
    pub mnemonic: String,
}

impl Default for EthPkgKurtosis {
    fn default() -> Self {
        Self::builder().build()
    }
}

pub const PRESENT_MINIMAL: &str = "minimal";
pub const PRESENT_MAINNET: &str = "mainnet";

pub fn get_service_port<'a>(
    service_info: impl Iterator<Item = (&'a String, &'a ServiceInfo)>,
    predicate: fn(&str) -> bool,
    port_name: &str,
) -> Option<(SocketAddr, SocketAddr)> {
    for (service_id, info) in service_info {
        if predicate(service_id) {
            if let Some(public_port_info) = info.maybe_public_ports.get(port_name) {
                let private_ip = &info.private_ip_addr;
                let public_ip = &info.maybe_public_ip_addr;
                let private_port_info = &info.private_ports[port_name];
                let private_socket = SocketAddr::new(
                    private_ip.parse().unwrap(),
                    private_port_info.number.try_into().unwrap(),
                );
                let public_socket = SocketAddr::new(
                    public_ip.parse().unwrap(),
                    public_port_info.number.try_into().unwrap(),
                );
                return Some((private_socket, public_socket));
            }
        }
    }

    None
}

impl EthereumNetwork for EthPkgKurtosis {
    async fn start(&mut self) -> TestResult {
        let enclave_name = self.enclave_name.clone();

        // CONNECT TO ENGINE
        let mut engine =
            EngineServiceClient::connect(format!("http://{}", self.kurtosis_engine_endpoint))
                .await?;

        // DESTROY ENCLAVE IF EXISTS
        if engine
            .get_enclaves(())
            .await?
            .into_inner()
            .enclave_info
            .values()
            .any(|x| x.name == enclave_name)
        {
            engine
                .destroy_enclave(DestroyEnclaveArgs {
                    enclave_identifier: enclave_name.clone(),
                })
                .await?;
        }

        // CREATE ENCLAVE
        let create_enclave_response = engine
            .create_enclave(CreateEnclaveArgs {
                enclave_name: Some(enclave_name.clone()),
                api_container_log_level: Some("info".to_string()),
                api_container_version_tag: None,
                mode: None,
                should_apic_run_in_debug_mode: None,
            })
            .await?
            .into_inner();

        // CONNECT TO ENCLAVE
        let enclave_info = create_enclave_response
            .enclave_info
            .expect("Enclave info must be present");
        let enclave_port = enclave_info
            .api_container_host_machine_info
            .expect("Enclave host machine info must be present")
            .grpc_port_on_host_machine;

        let mut enclave =
            ApiContainerServiceClient::connect(format!("https://[::1]:{}", enclave_port)).await?;

        // finality doesn't work with lighthouse (default)
        // transaction indexing with geth (default)
        let config = json!({
            "participants": [{
                "cl_type": "lodestar",
                "el_type": "reth",
                "el_extra_params": ["--rpc.eth-proof-window=512"]
            }],
            "network_params": {
                "network": "kurtosis",
                "preset": PRESENT_MINIMAL,
                "seconds_per_slot": self.block_time,
                "preregistered_validator_keys_mnemonic": self.mnemonic,
            },
            "wait_for_finalization": true,
        });

        // RUN STARLARK PACKAGE
        let mut run_result = enclave
            .run_starlark_package(RunStarlarkPackageArgs {
                package_id: "github.com/ethpandaops/ethereum-package".to_string(),
                serialized_params: Some(config.to_string()),
                dry_run: None,
                parallelism: None,
                clone_package: Some(true),
                relative_path_to_main_file: None,
                main_function_name: None,
                experimental_features: vec![],
                cloud_instance_id: None,
                cloud_user_id: None,
                image_download_mode: Some(ImageDownloadMode::Missing.into()),
                non_blocking_mode: Some(true),
                github_auth_token: std::env::var("GITHUB_TOKEN").ok(),
                starlark_package_content: None,
            })
            .await?
            .into_inner();

        // GET OUTPUT LINES WITH TIMEOUT
        while let Some(next_message) = run_result.message().await? {
            match next_message.run_response_line {
                Some(RunResponseLine::InstructionResult(result)) => {
                    println!("{}", result.serialized_instruction_result);
                }
                Some(RunResponseLine::RunFinishedEvent(result)) => {
                    if !result.is_run_successful {
                        return Err("Kurtosis run failed".into());
                    }
                    if let Some(output) = result.serialized_output {
                        println!("Output: {}", output);
                    }
                    break;
                }
                _ => continue,
            }
        }

        let resp = enclave
            .get_services(GetServicesArgs {
                service_identifiers: Default::default(),
            })
            .await?
            .into_inner();

        let el_socket = get_service_port(
            resp.service_info.iter(),
            |service_id| service_id.starts_with("el-"),
            "rpc",
        )
        .context("Failed to get el endpoint")?;

        let cl_socket = get_service_port(
            resp.service_info.iter(),
            |service_id| service_id.starts_with("cl-"),
            "http",
        )
        .context("Failed to get cl endpoint")?;

        println!("EL: {:?}", el_socket);
        println!("CL: {:?}", cl_socket);

        self.el_socket = Some(el_socket.1);
        self.cl_socket = Some(cl_socket.1);

        Ok(())
    }

    fn network_config(&self) -> EthereumConfig {
        EthereumConfig {
            el_socket: self.el_socket.unwrap(),
            cl_socket: self.cl_socket,
            mnemonics: vec![self.mnemonic.clone()],
            block_time: self.block_time,
        }
    }

    async fn stop(self) -> TestResult {
        let enclave_name = self.enclave_name.clone();

        // CONNECT TO ENGINE
        let mut engine =
            EngineServiceClient::connect(format!("http://{}", self.kurtosis_engine_endpoint))
                .await?;

        // DESTROY ENCLAVE
        engine
            .destroy_enclave(DestroyEnclaveArgs {
                enclave_identifier: enclave_name,
            })
            .await?;

        Ok(())
    }
}
