//! # Helpers for command execution


use super::Error;
use super::{CmdExecutor, Address, PrivateKey, KeyFile, trim_hex, to_arr, align_bytes};
use std::path::{Path, PathBuf};
use std::io::{self, Read, Write};
use std::fs::File;
use std::str::FromStr;
use std::env;
use rpc::{ClientMethod, MethodParams};
use jsonrpc_core::Params;
use serde_json::{Map, Value};
use rustc_serialize::hex::ToHex;
use hex::FromHex;


#[macro_export]
macro_rules! arg_to_opt {
    ( $arg:expr ) => {{
        let str = $arg.parse::<String>()?;
        if str.is_empty() {
            None
        } else {
            Some(str)
        }
    }};
}

macro_rules! arg_to_address {
    ( $arg:expr ) => {{
        let str = $arg.parse::<String>()?;
        Address::from_str(&str)?
    }};
}


/// Environment variables used to change default variables
#[derive(Default, Debug)]
pub struct EnvVars {
    pub emerald_base_path: Option<String>,
    pub emerald_host: Option<String>,
    pub emerald_port: Option<String>,
    pub emerald_chain: Option<String>,
    pub emerald_chain_id: Option<String>,
    pub emerald_gas: Option<String>,
    pub emerald_gas_price: Option<String>,
    pub emerald_security_level: Option<String>,
    pub emerald_node: Option<String>,
}

impl EnvVars {
    /// Collect environment variables to overwrite default values
    pub fn parse() -> EnvVars {
        let mut vars = EnvVars::default();
        for (key, value) in env::vars() {
            match key.as_ref() {
                "EMERALD_BASE_PATH" => vars.emerald_base_path = Some(value),
                "EMERALD_HOST" => vars.emerald_host = Some(value),
                "EMERALD_PORT" => vars.emerald_port = Some(value),
                "EMERALD_CHAIN" => vars.emerald_chain = Some(value),
                "EMERALD_CHAIN_ID" => vars.emerald_chain_id = Some(value),
                "EMERALD_GAS" => vars.emerald_gas = Some(value),
                "EMERALD_GAS_PRICE" => vars.emerald_gas_price = Some(value),
                "EMERALD_SECURITY_LEVEL" => vars.emerald_security_level = Some(value),
                "EMERALD_NODE" => vars.emerald_node = Some(value),
                _ => (),
            }
        }
        vars
    }
}

/// Try to parse argument
/// If no `arg` was supplied, try to get environment variable
///
/// # Arguments:
///
/// * arg - provided argument
/// * env - optional environment variable
///
pub fn arg_or_default(arg: &str, env: &Option<String>) -> Result<String, Error> {
    let val = arg.parse::<String>()?;

    if val.is_empty() {
        env.clone().ok_or_else(|| {
            Error::ExecError("Missed arguments".to_string())
        })
    } else {
        Ok(val)
    }
}

fn hex_to_32bytes(hex: &str) -> Result<[u8; 32], Error> {
    let bytes = Vec::from_hex(hex)?;
    Ok(to_arr(&align_bytes(&bytes, 32)))
}

impl CmdExecutor {
    /// Import Keyfile into storage
    pub fn import_keyfile<P: AsRef<Path>>(&self, path: P) -> Result<(), Error> {
        let mut json = String::new();
        File::open(path).and_then(
            |mut f| f.read_to_string(&mut json),
        )?;

        let kf = KeyFile::decode(json)?;
        self.storage.put(&kf)?;
        //        io::stdout().write_all(
        //            &format!("kf: {:?}\n", kf).into_bytes(),
        //        )?;

        Ok(())
    }

    /// Parse `address` from input arguments
    pub fn parse_address(&self) -> Result<Address, Error> {
        Ok(arg_to_address!(self.args.arg_address))
    }

    /// Parse `from` address for transaction signing
    pub fn parse_from(&self) -> Result<Address, Error> {
        Ok(arg_to_address!(self.args.arg_from))
    }

    /// Parse `to` address for transaction signing
    pub fn parse_to(&self) -> Result<Option<Address>, Error> {
        let str = self.args.arg_to.parse::<String>()?;
        let val = if str.is_empty() {
            None
        } else {
            Some(Address::from_str(&str)?)
        };

        Ok(val)
    }

    /// Parse private key for account creation
    pub fn parse_pk(&self) -> Result<PrivateKey, Error> {
        let pk_str = self.args.arg_path.parse::<String>()?;
        let pk = PrivateKey::from_str(&pk_str)?;

        Ok(pk)
    }

    /// Parse path for accounts import/export
    pub fn parse_path(&self) -> Result<PathBuf, Error> {
        let pk_str = arg_or_default(&self.args.arg_path, &self.vars.emerald_base_path)?;
        let pk = PathBuf::from(&pk_str);

        Ok(pk)
    }

    /// Parse gas limit for transaction execution
    pub fn parse_gas(&self) -> Result<u64, Error> {
        let gas_str = arg_or_default(&self.args.flag_gas, &self.vars.emerald_gas)?;
        let gas = trim_hex(&gas_str).parse::<u64>()?;

        Ok(gas)
    }

    /// Parse gas limit for transaction execution
    pub fn parse_gas_price(&self) -> Result<[u8; 32], Error> {
        let gp_str = arg_or_default(&self.args.flag_gas_price, &self.vars.emerald_gas_price)?;
        hex_to_32bytes(&gp_str)
    }

    /// Parse transaction value
    pub fn parse_value(&self) -> Result<[u8; 32], Error> {
        let value_str = self.args.arg_value.parse::<String>()?;
        hex_to_32bytes(&value_str)
    }

    /// Parse transaction data
    pub fn parse_data(&self) -> Result<Vec<u8>, Error> {
        let str = self.args.arg_value.parse::<String>()?;
        let data = Vec::from_hex(trim_hex(&str))?;
        Ok(data)
    }

    /// Request passphrase
    pub fn request_passphrase() -> Result<String, Error> {
        let mut out = io::stdout();
        out.write_all(b"Enter passphrase: \n")?;
        out.flush()?;

        let mut passphrase = String::new();
        io::stdin().read_line(&mut passphrase)?;

        Ok(passphrase)
    }

    /// Get nonce for address from remote node
    ///
    /// # Arguments:
    ///
    /// * addr - target address
    ///
    pub fn get_nonce(&self, addr: &Address) -> Result<u64, Error> {
        match self.connector {
            Some(ref conn) => {
                let data = vec![
                    Value::String(addr.to_string()),
                    Value::String("latest".to_string()),
                ];
                let params = Params::Array(data);

                let val = conn.send_post(
                    &MethodParams(ClientMethod::EthGetTxCount, &params),
                )?;
                match val.as_str() {
                    Some(s) => Ok(u64::from_str_radix(trim_hex(s), 16)?),
                    None => Err(Error::ExecError("Can't parse tx count".to_string())),
                }
            }
            None => Err(Error::ExecError("Can't connect to client".to_string())),
        }
    }


    /// Send signed raw transaction to the remote client
    ///
    /// # Arguments:
    ///
    /// * raw - signed tx
    ///
    /// # Return:
    ///
    /// * String - transaction hash
    ///
    pub fn send_transaction(&self, raw: Vec<u8>) -> Result<String, Error> {
        match self.connector {
            Some(ref conn) => {
                let mut data = Map::new();
                data.insert(
                    "data".to_string(),
                    Value::String(format!("0x{}", raw.to_hex())),
                );
                let params = Params::Map(data);

                conn.send_post(&MethodParams(ClientMethod::EthSendRawTransaction, &params))
                    .and_then(|v| match v.as_str() {
                        Some(str) => Ok(str.to_string()),
                        None => Err(Error::ExecError("Can't parse tx hash".to_string())),
                    })
            }
            None => Err(Error::ExecError("Can't connect to client".to_string())),
        }
    }
}
