use std::path::PathBuf;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyModule;
use tokio::runtime::Runtime;

use crate::{
    ClientOptions, RocketLeagueStatsClient, parse_stats_event,
    stats_event_name, stats_event_to_value,
};

#[pyclass(name = "RocketLeagueStatsClient", unsendable)]
pub struct PyRocketLeagueStatsClient {
    options: ClientOptions,
    runtime: Runtime,
    client: Option<RocketLeagueStatsClient>,
}

#[pymethods]
impl PyRocketLeagueStatsClient {
    #[new]
    #[pyo3(signature = (
        host = "127.0.0.1".to_string(),
        port = 49123,
        ini_path = None,
        auto_enable_packet_rate = true,
        packet_send_rate = 60.0,
        set_packet_rate_only_when_zero = true
    ))]
    fn new(
        host: String,
        port: u16,
        ini_path: Option<String>,
        auto_enable_packet_rate: bool,
        packet_send_rate: f32,
        set_packet_rate_only_when_zero: bool,
    ) -> PyResult<Self> {
        let mut options = ClientOptions::default();
        options.host = host;
        options.port_override = Some(port);
        options.auto_enable_packet_rate = auto_enable_packet_rate;
        options.packet_send_rate = packet_send_rate;
        options.set_packet_rate_only_when_zero = set_packet_rate_only_when_zero;
        options.stats_api_ini_path = ini_path.map(PathBuf::from);

        let runtime = Runtime::new().map_err(to_runtime_err)?;

        Ok(Self {
            options,
            runtime,
            client: None,
        })
    }

    fn connect(&mut self) -> PyResult<()> {
        let client = self
            .runtime
            .block_on(RocketLeagueStatsClient::connect(self.options.clone()))
            .map_err(to_runtime_err)?;

        self.client = Some(client);
        Ok(())
    }

    fn reconnect(&mut self) -> PyResult<()> {
        if self.client.is_none() {
            return self.connect();
        }

        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client not connected"))?;
        self.runtime
            .block_on(client.reconnect())
            .map_err(to_runtime_err)
    }

    fn next_event_json(&mut self) -> PyResult<Option<String>> {
        self.ensure_connected()?;

        let client = self
            .client
            .as_mut()
            .ok_or_else(|| PyRuntimeError::new_err("client not connected"))?;

        let event = self
            .runtime
            .block_on(client.next_event())
            .map_err(to_runtime_err)?;

        match event {
            Some(event) => {
                let value = stats_event_to_value(&event).map_err(to_runtime_err)?;
                let serialized =
                    serde_json::to_string(&value).map_err(to_runtime_err)?;
                Ok(Some(serialized))
            }
            None => Ok(None),
        }
    }

    fn close(&mut self) -> PyResult<()> {
        if let Some(client) = self.client.take() {
            self.runtime
                .block_on(client.close())
                .map_err(to_runtime_err)?;
        }

        Ok(())
    }

    fn socket_address(&self) -> String {
        let host = &self.options.host;
        let port = self.options.port_override.unwrap_or(49123);
        format!("{host}:{port}")
    }
}

impl PyRocketLeagueStatsClient {
    fn ensure_connected(&mut self) -> PyResult<()> {
        if self.client.is_none() {
            self.connect()?;
        }

        Ok(())
    }
}

#[pyfunction]
fn parse_event_json(raw: &str) -> PyResult<String> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    let value = stats_event_to_value(&event).map_err(to_value_err)?;
    serde_json::to_string(&value).map_err(to_value_err)
}

#[pyfunction]
fn event_name(raw: &str) -> PyResult<String> {
    let event = parse_stats_event(raw).map_err(to_value_err)?;
    Ok(stats_event_name(&event).to_string())
}

#[pymodule]
fn rlstatsapi(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRocketLeagueStatsClient>()?;
    m.add_function(wrap_pyfunction!(parse_event_json, m)?)?;
    m.add_function(wrap_pyfunction!(event_name, m)?)?;
    Ok(())
}

fn to_runtime_err<E: std::fmt::Display>(error: E) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

fn to_value_err<E: std::fmt::Display>(error: E) -> PyErr {
    PyValueError::new_err(error.to_string())
}
