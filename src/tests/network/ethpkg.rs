use super::{EthereumConfig, EthereumNetwork};
use anyhow::Context;
use bon::Builder;
use core::net::Ipv4Addr;
use core::net::SocketAddr;
use kurtosis_sdk::enclave_api::starlark_run_response_line::RunResponseLine;
use kurtosis_sdk::enclave_api::{GetServicesArgs, ServiceInfo};
use kurtosis_sdk::{
    enclave_api::{
        api_container_service_client::ApiContainerServiceClient, ImageDownloadMode,
        RunStarlarkPackageArgs,
    },
    engine_api::{
        engine_service_client::EngineServiceClient, CreateEnclaveArgs, DestroyEnclaveArgs,
    },
};
use serde_json::json;
use testresult::TestResult;

#[derive(Builder, Debug)]
pub struct EthPkgKurtosis {
    #[builder(default = "ethpkg".into())]
    pub enclave_name: String,
    pub el_socket: Option<SocketAddr>,
    pub cl_socket: Option<SocketAddr>,
    #[builder(default = 12)]
    pub block_time: u64,
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
            EngineServiceClient::connect(format!("http://{}:{}", Ipv4Addr::UNSPECIFIED, 9710))
                .await?;

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

        // Create the configuration for a reth + lighthouse network
        let config = json!({
            "participants": [
                {
                    "el_type": "reth",
                    "cl_type": "lighthouse",
                    "count": 1,
                    "use_separate_vc": true,
                    "vc_type": "lighthouse"
                }
            ],
            "network_params": {
                "network": "kurtosis",
                // "preset": "mainnet",
                "preset": "minimal",
                "seconds_per_slot": self.block_time,
                "num_validator_keys_per_node": 64,
                "deneb_fork_epoch": 0
            },
            // "additional_services": [
            //     "prometheus_grafana"
            // ],
            // "wait_for_finalization": true,
            "global_log_level": "info",
            "port_publisher": {
                "el": {"enabled": true},
                "cl": {"enabled": true},
                "vc": {"enabled": true}
            }
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
                github_auth_token: None,
                starlark_package_content: None,
            })
            .await?
            .into_inner();

        // GET OUTPUT LINES WITH TIMEOUT
        let result = tokio::time::timeout(tokio::time::Duration::from_secs(120), async {
            while let Some(next_message) = run_result.message().await? {
                match next_message.run_response_line {
                    Some(RunResponseLine::InstructionResult(result)) => {
                        println!("{}", result.serialized_instruction_result);
                    }
                    Some(RunResponseLine::RunFinishedEvent(result)) => {
                        println!("Run finished: {:#?}", result);
                        break;
                    }
                    _ => continue,
                }
            }
            Ok::<(), anyhow::Error>(())
        })
        .await;

        match result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => {
                println!("Timeout occurred");
                return Err("Operation timed out".into());
            }
        }

        let resp = enclave
            .get_services(GetServicesArgs {
                service_identifiers: Default::default(),
            })
            .await?
            .into_inner();

        println!("Services: {:#?}", resp);

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
            ip: self.el_socket.unwrap().ip(),
            port: self.el_socket.unwrap().port(),
            mnemonics: vec!["giant issue aisle success illegal bike spike question tent bar rely arctic volcano long crawl hungry vocal artwork sniff fantasy very lucky have athlete".into()],
            block_time: self.block_time,
        }
    }

    async fn stop(self) -> TestResult {
        let enclave_name = self.enclave_name.clone();

        // CONNECT TO ENGINE
        let mut engine =
            EngineServiceClient::connect(format!("http://{}:{}", Ipv4Addr::UNSPECIFIED, 9710))
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
