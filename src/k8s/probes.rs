/// Port-forward based probe execution for health and readiness checks
use crate::error::Result;
use crate::tui::data::ContainerProbe;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;
use tracing::{debug, error};

/// Result of executing a probe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub probe_type: String,        // "Liveness", "Readiness", "Startup"
    pub handler_type: String,      // "HTTP", "TCP", "Exec"
    pub status: ProbeStatus,       // Success, Failure, Timeout, Error
    pub response_time_ms: u64,     // How long the probe took
    pub status_code: Option<u16>,  // HTTP status code if applicable
    pub response_body: String,     // Response content (truncated)
    pub error_message: Option<String>, // Error details if failed
    pub timestamp: String,         // When the probe was executed
}

/// Status of a probe execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProbeStatus {
    Success,    // Probe succeeded (2xx for HTTP)
    Failure,    // Probe failed (non-2xx for HTTP, connection refused, etc.)
    Timeout,    // Probe timed out
    Error,      // Error executing probe (invalid config, etc.)
}

/// Port-forward manager for probe execution
pub struct ProbeExecutor {
    pod_name: String,
    namespace: String,
}

impl ProbeExecutor {
    /// Create a new probe executor for a specific pod
    #[must_use]
    pub const fn new(pod_name: String, namespace: String) -> Self {
        Self { pod_name, namespace }
    }
    
    /// Execute a probe and return the result
    pub async fn execute_probe(&self, probe: &ContainerProbe) -> ProbeResult {
        let start_time = Instant::now();
        let timestamp = chrono::Utc::now().format("%H:%M:%S").to_string();
        
        debug!("Executing {} probe for pod {}: {}", probe.probe_type, self.pod_name, probe.details);
        
        let result = match probe.handler_type.as_str() {
            "HTTP" => self.execute_http_probe(probe).await,
            "TCP" => self.execute_tcp_probe(probe).await,
            "Exec" => self.execute_exec_probe(probe).await,
            _ => ProbeResult {
                probe_type: probe.probe_type.clone(),
                handler_type: probe.handler_type.clone(),
                status: ProbeStatus::Error,
                response_time_ms: 0,
                status_code: None,
                response_body: String::new(),
                error_message: Some(format!("Unsupported probe handler type: {}", probe.handler_type)),
                timestamp: timestamp.clone(),
            }
        };
        
        let elapsed = start_time.elapsed();
        let mut final_result = result;
        final_result.response_time_ms = elapsed.as_millis().try_into().unwrap_or(u64::MAX);
        final_result.timestamp = timestamp;
        
        debug!("Probe {} completed in {}ms with status {:?}", 
               probe.probe_type, final_result.response_time_ms, final_result.status);
        
        final_result
    }
    
    /// Execute an HTTP probe via port-forward
    async fn execute_http_probe(&self, probe: &ContainerProbe) -> ProbeResult {
        // Parse HTTP probe details: "GET http://localhost:8080/health"
        let (method, url) = self.parse_http_details(&probe.details);
        
        // Extract port from URL or probe details
        let port = self.extract_port_from_url(&url)
            .or_else(|| self.extract_port_from_details(&probe.details))
            .unwrap_or(8080); // Default port
            
        let path = self.extract_path_from_details(&probe.details);
        
        // Create port-forward if needed
        match self.ensure_port_forward(port).await {
            Ok(local_port) => {
                // Execute HTTP request
                let local_url = format!("http://localhost:{local_port}{path}");
                self.execute_http_request(&method, &local_url, probe).await
            }
            Err(e) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Error,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: String::new(),
                    error_message: Some(format!("Port-forward failed: {e}")),
                    timestamp: String::new(),
                }
            }
        }
    }
    
    /// Execute a TCP probe via port-forward
    async fn execute_tcp_probe(&self, probe: &ContainerProbe) -> ProbeResult {
        // Parse TCP probe details: "Connect to localhost:8080"
        let port = self.extract_port_from_details(&probe.details).unwrap_or(8080);
        
        match self.ensure_port_forward(port).await {
            Ok(local_port) => {
                // Test TCP connection
                self.test_tcp_connection(local_port, probe).await
            }
            Err(e) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Error,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: String::new(),
                    error_message: Some(format!("Port-forward failed: {e}")),
                    timestamp: String::new(),
                }
            }
        }
    }
    
    /// Execute an Exec probe (limited support - just return the command)
    async fn execute_exec_probe(&self, probe: &ContainerProbe) -> ProbeResult {
        ProbeResult {
            probe_type: probe.probe_type.clone(),
            handler_type: probe.handler_type.clone(),
            status: ProbeStatus::Error,
            response_time_ms: 0,
            status_code: None,
            response_body: format!("Exec probe not supported via port-forward: {}", probe.details),
            error_message: Some("Exec probes require direct pod access".to_string()),
            timestamp: String::new(),
        }
    }
    
    /// Ensure a port-forward exists for the given port
    async fn ensure_port_forward(&self, remote_port: u16) -> Result<u16> {
        // For simplicity, use the same port locally as remotely
        // In a production implementation, you might want to manage a pool of local ports
        let local_port = remote_port;
        
        // Check if port-forward is already running by testing the connection
        if self.test_local_port(local_port).await {
            debug!("Port-forward already exists for port {}", local_port);
            return Ok(local_port);
        }
        
        // Start new port-forward
        debug!("Starting port-forward for pod {} port {} -> {}", self.pod_name, remote_port, local_port);
        
        let mut cmd = Command::new("kubectl");
        cmd.args([
            "port-forward",
            &format!("pod/{}", self.pod_name),
            &format!("{local_port}:{remote_port}"),
            "-n", &self.namespace,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        
        // Start the port-forward process in the background
        match cmd.spawn() {
            Ok(mut child) => {
                // Give it a moment to establish
                tokio::time::sleep(Duration::from_millis(500)).await;
                
                // Test if it's working
                if self.test_local_port(local_port).await {
                    debug!("Port-forward established successfully for port {}", local_port);
                    Ok(local_port)
                } else {
                    // Kill the child process if it didn't work
                    let _ = child.kill().await;
                    Err(crate::error::Error::Custom(format!("Port-forward failed to establish for port {local_port}")))
                }
            }
            Err(e) => {
                error!("Failed to start kubectl port-forward: {}", e);
                Err(e.into())
            }
        }
    }
    
    /// Test if a local port is responding
    async fn test_local_port(&self, port: u16) -> bool {
        (tokio::net::TcpStream::connect(format!("127.0.0.1:{port}")).await).is_ok()
    }
    
    /// Execute HTTP request to localhost
    async fn execute_http_request(&self, method: &str, url: &str, probe: &ContainerProbe) -> ProbeResult {
        let client = reqwest::Client::new();
        let timeout_duration = Duration::from_secs(probe.timeout as u64);
        
        let request = match method {
            "GET" => client.get(url),
            "POST" => client.post(url),
            "HEAD" => client.head(url),
            _ => client.get(url), // Default to GET
        };
        
        match timeout(timeout_duration, request.send()).await {
            Ok(Ok(response)) => {
                let status_code = response.status().as_u16();
                let is_success = response.status().is_success();
                
                // Read response body (limit to 1KB for display)
                let body = match response.text().await {
                    Ok(text) => {
                        if text.len() > 1024 {
                            format!("{}...", &text[..1024])
                        } else {
                            text
                        }
                    }
                    Err(_) => "[Failed to read response body]".to_string(),
                };
                
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: if is_success { ProbeStatus::Success } else { ProbeStatus::Failure },
                    response_time_ms: 0, // Will be set by caller
                    status_code: Some(status_code),
                    response_body: body,
                    error_message: if is_success { None } else { Some(format!("HTTP {status_code}")) },
                    timestamp: String::new(),
                }
            }
            Ok(Err(e)) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Failure,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: String::new(),
                    error_message: Some(format!("HTTP request failed: {e}")),
                    timestamp: String::new(),
                }
            }
            Err(_) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Timeout,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: String::new(),
                    error_message: Some(format!("Request timed out after {}s", probe.timeout)),
                    timestamp: String::new(),
                }
            }
        }
    }
    
    /// Test TCP connection
    async fn test_tcp_connection(&self, port: u16, probe: &ContainerProbe) -> ProbeResult {
        let timeout_duration = Duration::from_secs(probe.timeout as u64);
        
        match timeout(timeout_duration, tokio::net::TcpStream::connect(format!("127.0.0.1:{port}"))).await {
            Ok(Ok(_)) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Success,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: format!("TCP connection to port {port} successful"),
                    error_message: None,
                    timestamp: String::new(),
                }
            }
            Ok(Err(e)) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Failure,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: String::new(),
                    error_message: Some(format!("TCP connection failed: {e}")),
                    timestamp: String::new(),
                }
            }
            Err(_) => {
                ProbeResult {
                    probe_type: probe.probe_type.clone(),
                    handler_type: probe.handler_type.clone(),
                    status: ProbeStatus::Timeout,
                    response_time_ms: 0,
                    status_code: None,
                    response_body: String::new(),
                    error_message: Some(format!("TCP connection timed out after {}s", probe.timeout)),
                    timestamp: String::new(),
                }
            }
        }
    }
    
    /// Parse HTTP details to extract method and URL
    fn parse_http_details(&self, details: &str) -> (String, String) {
        // Handle formats like:
        // "GET http://localhost:8080/health"
        // "/health"
        // "http://localhost:8080/health"
        
        if details.starts_with("GET ") || details.starts_with("POST ") || details.starts_with("HEAD ") {
            let parts: Vec<&str> = details.splitn(2, ' ').collect();
            if parts.len() == 2 {
                (parts[0].to_string(), parts[1].to_string())
            } else {
                ("GET".to_string(), details.to_string())
            }
        } else {
            ("GET".to_string(), details.to_string())
        }
    }
    
    /// Extract port from URL
    fn extract_port_from_url(&self, url: &str) -> Option<u16> {
        if let Some(start) = url.find("://") {
            let after_protocol = &url[start + 3..];
            if let Some(end) = after_protocol.find('/') {
                let host_port = &after_protocol[..end];
                if let Some(colon_pos) = host_port.rfind(':') {
                    return host_port[colon_pos + 1..].parse().ok();
                }
            } else if let Some(colon_pos) = after_protocol.rfind(':') {
                return after_protocol[colon_pos + 1..].parse().ok();
            }
        }
        None
    }
    
    /// Extract port from probe details
    fn extract_port_from_details(&self, details: &str) -> Option<u16> {
        // Look for patterns like ":8080" or "port 8080"
        if let Some(colon_pos) = details.rfind(':') {
            let port_str = &details[colon_pos + 1..];
            // Take only digits
            let port_digits: String = port_str.chars().take_while(char::is_ascii_digit).collect();
            if !port_digits.is_empty() {
                return port_digits.parse().ok();
            }
        }
        None
    }
    
    /// Extract path from probe details
    fn extract_path_from_details(&self, details: &str) -> String {
        // Handle formats like:
        // "GET http://localhost:8080/health" -> "/health"
        // "/health" -> "/health"
        // "http://localhost:8080/health" -> "/health"
        
        if details.starts_with('/') {
            details.to_string()
        } else if let Some(protocol_pos) = details.find("://") {
            let after_protocol = &details[protocol_pos + 3..];
            if let Some(slash_pos) = after_protocol.find('/') {
                after_protocol[slash_pos..].to_string()
            } else {
                "/".to_string()
            }
        } else {
            "/".to_string()
        }
    }
}