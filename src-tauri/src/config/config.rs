use super::{Draft, IClashTemp, IProfiles, IRuntime, IVerge};
use crate::{
    config::PrfItem,
    enhance,
    utils::{dirs, help},
    core::{handle, CoreManager},
};
use anyhow::{anyhow, Result};
use once_cell::sync::OnceCell;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

pub const RUNTIME_CONFIG: &str = "clash-verge.yaml";
pub const CHECK_CONFIG: &str = "clash-verge-check.yaml";

pub struct Config {
    clash_config: Draft<IClashTemp>,
    verge_config: Draft<IVerge>,
    profiles_config: Draft<IProfiles>,
    runtime_config: Draft<IRuntime>,
}

impl Config {
    pub fn global() -> &'static Config {
        static CONFIG: OnceCell<Config> = OnceCell::new();

        CONFIG.get_or_init(|| Config {
            clash_config: Draft::from(IClashTemp::new()),
            verge_config: Draft::from(IVerge::new()),
            profiles_config: Draft::from(IProfiles::new()),
            runtime_config: Draft::from(IRuntime::new()),
        })
    }

    pub fn clash() -> Draft<IClashTemp> {
        Self::global().clash_config.clone()
    }

    pub fn verge() -> Draft<IVerge> {
        Self::global().verge_config.clone()
    }

    pub fn profiles() -> Draft<IProfiles> {
        Self::global().profiles_config.clone()
    }

    pub fn runtime() -> Draft<IRuntime> {
        Self::global().runtime_config.clone()
    }

    /// 初始化订阅
    pub async fn init_config() -> Result<()> {
        if Self::profiles()
            .data()
            .get_item(&"Merge".to_string())
            .is_err()
        {
            let merge_item = PrfItem::from_merge(Some("Merge".to_string()))?;
            Self::profiles().data().append_item(merge_item.clone())?;
        }
        if Self::profiles()
            .data()
            .get_item(&"Script".to_string())
            .is_err()
        {
            let script_item = PrfItem::from_script(Some("Script".to_string()))?;
            Self::profiles().data().append_item(script_item.clone())?;
        }

        // 生成运行时配置
        crate::log_err!(Self::generate().await);

        // 生成运行时配置文件并验证
        let config_result = Self::generate_file(ConfigType::Run);

        let validation_result = if let Ok(_) = config_result {
            // 验证配置文件
            println!("[首次启动] 开始验证配置");
            
            match CoreManager::global().validate_config().await {
                Ok((is_valid, error_msg)) => {
                    if !is_valid {
                        println!("[首次启动] 配置验证失败，使用默认最小配置启动{}", error_msg);
                        if error_msg.is_empty() {
                            CoreManager::global()
                                .use_default_config(
                                    "config_validate::boot_error",
                                    "",
                                )
                                .await?;
                            Some(("config_validate::boot_error", String::new()))
                        } else {
                            CoreManager::global()
                                .use_default_config(
                                    "config_validate::stderr_error",
                                    &error_msg,
                                )
                                .await?;
                            Some(("config_validate::stderr_error", error_msg))
                        }
                    } else {
                        println!("[首次启动] 配置验证成功");
                        Some(("config_validate::success", String::new()))
                    }
                }
                Err(err) => {
                    println!("[首次启动] 验证进程执行失败: {}", err);
                    CoreManager::global()
                        .use_default_config(
                            "config_validate::process_terminated",
                            "",
                        )
                        .await?;
                    Some(("config_validate::process_terminated", String::new()))
                }
            }
        } else {
            println!("[首次启动] 生成配置文件失败，使用默认配置");
            CoreManager::global()
                .use_default_config(
                    "config_validate::error",
                    "",
                )
                .await?;
            Some(("config_validate::error", String::new()))
        };

        // 在单独的任务中发送通知
        if let Some((msg_type, msg_content)) = validation_result {
            tauri::async_runtime::spawn(async move {
                sleep(Duration::from_secs(2)).await;
                handle::Handle::notice_message(msg_type, &msg_content);
            });
        }

        Ok(())
    }

    /// 将订阅丢到对应的文件中
    pub fn generate_file(typ: ConfigType) -> Result<PathBuf> {
        let path = match typ {
            ConfigType::Run => dirs::app_home_dir()?.join(RUNTIME_CONFIG),
            ConfigType::Check => dirs::app_home_dir()?.join(CHECK_CONFIG),
        };

        let runtime = Config::runtime();
        let runtime = runtime.latest();
        let config = runtime
            .config
            .as_ref()
            .ok_or(anyhow!("failed to get runtime config"))?;

        help::save_yaml(&path, &config, Some("# Generated by Clash Verge"))?;
        Ok(path)
    }

    /// 生成订阅存好
    pub async fn generate() -> Result<()> {
        let (config, exists_keys, logs) = enhance::enhance().await;

        *Config::runtime().draft() = IRuntime {
            config: Some(config),
            exists_keys,
            chain_logs: logs,
        };

        Ok(())
    }
}

#[derive(Debug)]
pub enum ConfigType {
    Run,
    Check,
}
