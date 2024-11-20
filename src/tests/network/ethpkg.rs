use super::{EthereumConfig, EthereumNetwork};
use bon::Builder;
use core::net::Ipv4Addr;
use kurtosis_sdk::enclave_api::starlark_run_response_line::RunResponseLine::InstructionResult;
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
use tokio::time::Duration;

#[derive(Builder, Debug)]
pub struct EthPkgKurtosis {
    #[builder(default = "ethpkg".into())]
    enclave_name: String,
}

impl Default for EthPkgKurtosis {
    fn default() -> Self {
        Self::builder().build()
    }
}

pub const PRESENT_MINIMAL: &str = "minimal";
pub const PRESENT_MAINNET: &str = "mainnet";

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
                "preset": "mainnet",
                "seconds_per_slot": 12,
                "num_validator_keys_per_node": 64,
                "deneb_fork_epoch": 0
            },
            "additional_services": [
                "prometheus_grafana"
            ],
            "wait_for_finalization": true,
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
                non_blocking_mode: None,
                github_auth_token: None,
                starlark_package_content: None,
            })
            .await?
            .into_inner();

        // GET OUTPUT LINES
        let _ = tokio::time::timeout(Duration::from_secs(30), async {
            while let Some(next_message) = run_result.message().await? {
                if let Some(InstructionResult(result)) = next_message.run_response_line {
                    println!("{}", result.serialized_instruction_result);
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await;

        Ok(())
    }

    fn network_config(&self) -> EthereumConfig {
        EthereumConfig {
            ip: Ipv4Addr::UNSPECIFIED.into(),
            port: 32002,
            mnemonics: vec!["giant issue aisle success illegal bike spike question tent bar rely arctic volcano long crawl hungry vocal artwork sniff fantasy very lucky have athlete".into()],
            block_time: 12,
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
