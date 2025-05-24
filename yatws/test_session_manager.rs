// gen_goldens/test_session_manager.rs

use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
use std::path::Path;
use yatws::{IBKRClient, IBKRError, SessionStatus, SessionInfo};
use log::{info, warn, error};
use rusqlite::{params, Connection as DbConnection};
use chrono::{DateTime, TimeZone, Utc};

/// Manages test session lifecycle separate from IBKRClient
/// Used only by gen_goldens test runner
pub struct TestSessionManager {
  db_path: String,
  run_id: String,
  client: Option<Arc<IBKRClient>>,
}

impl TestSessionManager {
  /// Create a new test session manager for a run
  pub fn new(db_path: &str, run_id: &str) -> Self {
    Self {
      db_path: db_path.to_string(),
      run_id: run_id.to_string(),
      client: None,
    }
  }

  /// Create client for this run and associate it with the manager
  pub fn create_client(&mut self, host: &str, port: u16, client_id: i32) -> Result<Arc<IBKRClient>, IBKRError> {
    info!("Creating client for run '{}' ({}:{})", self.run_id, host, port);

    // Create client with run_id as session name (parent session)
    let client = Arc::new(IBKRClient::new(host, port, client_id, Some((
      self.db_path.clone(),
      self.run_id.clone()
    )))?);

    self.client = Some(client.clone());
    info!("Client created successfully for run '{}'", self.run_id);
    Ok(client)
  }

  /// Start a test session (creates child session)
  pub fn start_test(&self, test_name: &str) -> Result<(), IBKRError> {
    info!("Starting test session: '{}'", test_name);

    if let Some(client) = &self.client {
      client.start_test_session(test_name)?;
      info!("Test session '{}' started successfully", test_name);
      Ok(())
    } else {
      Err(IBKRError::InternalError("No client associated with TestSessionManager".to_string()))
    }
  }

  /// End current test session
  pub fn end_test(&self, status: SessionStatus) -> Result<(), IBKRError> {
    info!("Ending current test session with status: {:?}", status);

    if let Some(client) = &self.client {
      client.end_test_session(status)?;
      info!("Test session ended successfully");
      Ok(())
    } else {
      Err(IBKRError::InternalError("No client associated with TestSessionManager".to_string()))
    }
  }

  /// Complete the entire run
  pub fn complete_run(&self, status: SessionStatus) -> Result<(), IBKRError> {
    info!("Completing run '{}' with status: {:?}", self.run_id, status);

    if let Some(client) = &self.client {
      client.complete_run(status)?;
      info!("Run '{}' completed successfully", self.run_id);
      Ok(())
    } else {
      Err(IBKRError::InternalError("No client associated with TestSessionManager".to_string()))
    }
  }

  /// Get the run ID for this session manager
  pub fn run_id(&self) -> &str {
    &self.run_id
  }

  /// Get the associated client (if any)
  pub fn client(&self) -> Option<Arc<IBKRClient>> {
    self.client.clone()
  }

  /// Find latest run for given run_id
  pub fn find_latest_run<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<Option<SessionInfo>, IBKRError> {
    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open database: {}", e)))?;

    let result = db.query_row(
      "SELECT id, session_name, parent_id, created_at, completed_at, status, host, port, client_id, server_version
             FROM sessions
             WHERE session_name = ?1 AND parent_id IS NULL
             ORDER BY created_at DESC LIMIT 1",
      params![run_id],
      |row| {
        let created_at_str: String = row.get(3)?;
        let completed_at_str: Option<String> = row.get(4)?;

        let created_at = DateTime::parse_from_rfc3339(&format!("{}Z", created_at_str))
          .map_err(|_| rusqlite::Error::InvalidColumnType(3, "DateTime".into(), rusqlite::types::Type::Text))?
          .with_timezone(&Utc);

        let completed_at = completed_at_str
          .map(|s| DateTime::parse_from_rfc3339(&format!("{}Z", s))
               .map(|dt| dt.with_timezone(&Utc)))
          .transpose()
          .map_err(|_| rusqlite::Error::InvalidColumnType(4, "DateTime".into(), rusqlite::types::Type::Text))?;

        Ok(SessionInfo {
          id: row.get(0)?,
          session_name: row.get(1)?,
          parent_id: row.get(2)?,
          created_at,
          completed_at,
          status: row.get(5)?,
          host: row.get(6)?,
          port: row.get(7)?,
          client_id: row.get(8)?,
          server_version: row.get(9)?,
        })
      },
    );

    match result {
      Ok(session_info) => Ok(Some(session_info)),
      Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
      Err(e) => Err(IBKRError::LoggingError(format!("Failed to query latest run '{}': {}", run_id, e))),
    }
  }

  /// Get available tests for latest run
  pub fn get_available_tests<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<Vec<String>, IBKRError> {
    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open database: {}", e)))?;

    // First find the parent session
    let parent_id: i64 = db.query_row(
      "SELECT id FROM sessions WHERE session_name = ?1 AND parent_id IS NULL ORDER BY created_at DESC LIMIT 1",
      params![run_id],
      |row| row.get(0),
    ).map_err(|e| match e {
      rusqlite::Error::QueryReturnedNoRows => IBKRError::InvalidParameter(format!("Run '{}' not found", run_id)),
      _ => IBKRError::LoggingError(format!("Failed to find parent session for run '{}': {}", run_id, e)),
    })?;

    // Get completed child sessions - properly scope stmt
    let test_names = {
      let mut stmt = db.prepare(
        "SELECT session_name FROM sessions
                 WHERE parent_id = ?1 AND status = 'completed'
                 ORDER BY created_at ASC"
      ).map_err(|e| IBKRError::LoggingError(format!("Failed to prepare query for completed tests: {}", e)))?;

      let names: Result<Vec<String>, _> = stmt.query_map(params![parent_id], |row| {
        Ok(row.get::<_, String>(0)?)
      }).map_err(|e| IBKRError::LoggingError(format!("Failed to query completed tests: {}", e)))?
        .collect();

      names.map_err(|e| IBKRError::LoggingError(format!("Failed to collect completed tests: {}", e)))?
    }; // stmt is dropped here

    Ok(test_names)
  }

  /// Create replay client for latest run (all completed tests)
  pub fn replay_latest_run<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<IBKRClient, IBKRError> {
    info!("Creating replay client for latest run: '{}'", run_id);

    let parent_info = Self::find_latest_run(&db_path, run_id)?
      .ok_or_else(|| IBKRError::InvalidParameter(format!("Run '{}' not found", run_id)))?;

    info!("Found parent session: {} (created: {})", parent_info.session_name, parent_info.created_at);

    // Use the new public API method
    IBKRClient::from_full_run(db_path, run_id)
  }

  /// Create replay client for specific test
  pub fn replay_test<P: AsRef<Path>>(db_path: P, run_id: &str, test_name: &str) -> Result<IBKRClient, IBKRError> {
    info!("Creating replay client for test '{}' in run '{}'", test_name, run_id);

    // Use the new public API method
    IBKRClient::from_run_and_test(db_path, run_id, test_name)
  }

  /// Clear all sessions for a run_id (parent + all children)
  pub fn clear_run_sessions<P: AsRef<Path>>(db_path: P, run_id: &str) -> Result<(), IBKRError> {
    info!("Clearing all sessions for run: '{}'", run_id);

    let mut db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open database: {}", e)))?;

    let deleted_count = db.execute(
      "DELETE FROM sessions WHERE session_name = ?1 AND parent_id IS NULL",
      params![run_id],
    ).map_err(|e| IBKRError::LoggingError(format!("Failed to clear run '{}': {}", run_id, e)))?;

    if deleted_count > 0 {
      info!("Cleared {} run session(s) named '{}'", deleted_count, run_id);
    }

    Ok(())
  }

  /// Find latest run with a specific test (searches across all runs)
  pub fn find_latest_run_with_test<P: AsRef<Path>>(db_path: P, test_name: &str) -> Result<Option<String>, IBKRError> {
    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open database: {}", e)))?;

    // Find the most recent parent session that has a completed child with test_name
    let result = db.query_row(
      "SELECT p.session_name
             FROM sessions p
             INNER JOIN sessions c ON c.parent_id = p.id
             WHERE p.parent_id IS NULL
               AND c.session_name = ?1
               AND c.status = 'completed'
             ORDER BY p.created_at DESC
             LIMIT 1",
      params![test_name],
      |row| Ok(row.get::<_, String>(0)?)
    );

    match result {
      Ok(run_id) => Ok(Some(run_id)),
      Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
      Err(e) => Err(IBKRError::LoggingError(format!("Failed to find run with test '{}': {}", test_name, e))),
    }
  }

  /// Auto-discover and create replay client for a test from any available run
  pub fn auto_replay_test<P: AsRef<Path>>(db_path: P, test_name: &str) -> Result<IBKRClient, IBKRError> {
    info!("Auto-discovering replay client for test: '{}'", test_name);

    // First try to find the test as a standalone run
    if let Ok(Some(_)) = Self::find_latest_run(&db_path, test_name) {
      info!("Found '{}' as standalone run, replaying directly", test_name);
      return IBKRClient::from_db(db_path.as_ref().to_string_lossy().as_ref(), test_name);
    }

    // Then try to find it as part of another run
    if let Ok(Some(run_id)) = Self::find_latest_run_with_test(&db_path, test_name) {
      info!("Found '{}' in run '{}', replaying combined session", test_name, run_id);
      return Self::replay_test(db_path, &run_id, test_name);
    }

    Err(IBKRError::InvalidParameter(format!("Test '{}' not found in any recorded sessions", test_name)))
  }

  /// Get statistics about available runs and tests
  pub fn get_session_stats<P: AsRef<Path>>(db_path: P) -> Result<SessionStats, IBKRError> {
    let db = DbConnection::open(db_path)
      .map_err(|e| IBKRError::ConfigurationError(format!("Failed to open database: {}", e)))?;

    // Count parent sessions (runs)
    let total_runs: i64 = db.query_row(
      "SELECT COUNT(*) FROM sessions WHERE parent_id IS NULL",
      [],
      |row| row.get(0)
    ).unwrap_or(0);

    // Count completed child sessions (tests)
    let total_tests: i64 = db.query_row(
      "SELECT COUNT(*) FROM sessions WHERE parent_id IS NOT NULL AND status = 'completed'",
      [],
      |row| row.get(0)
    ).unwrap_or(0);

    // Count failed child sessions
    let failed_tests: i64 = db.query_row(
      "SELECT COUNT(*) FROM sessions WHERE parent_id IS NOT NULL AND status = 'failed'",
      [],
      |row| row.get(0)
    ).unwrap_or(0);

    // Get unique test names - ensure stmt is scoped properly
    let unique_tests = {
      let mut stmt = db.prepare(
        "SELECT DISTINCT session_name FROM sessions
                 WHERE parent_id IS NOT NULL AND status = 'completed'
                 ORDER BY session_name"
      ).map_err(|e| IBKRError::LoggingError(format!("Failed to prepare unique tests query: {}", e)))?;

      let test_names: Result<Vec<String>, _> = stmt.query_map([], |row| {
        Ok(row.get::<_, String>(0)?)
      }).map_err(|e| IBKRError::LoggingError(format!("Failed to query unique tests: {}", e)))?
        .collect();

      test_names.map_err(|e| IBKRError::LoggingError(format!("Failed to collect unique tests: {}", e)))?
    }; // stmt is dropped here

    Ok(SessionStats {
      total_runs: total_runs as usize,
      total_completed_tests: total_tests as usize,
      total_failed_tests: failed_tests as usize,
      unique_test_names: unique_tests,
    })
  }
}

/// Statistics about available sessions in the database
#[derive(Debug, Clone)]
pub struct SessionStats {
  pub total_runs: usize,
  pub total_completed_tests: usize,
  pub total_failed_tests: usize,
  pub unique_test_names: Vec<String>,
}

impl SessionStats {
  pub fn summary(&self) -> String {
    format!(
      "Runs: {}, Completed Tests: {}, Failed Tests: {}, Unique Tests: {}",
      self.total_runs, self.total_completed_tests, self.total_failed_tests, self.unique_test_names.len()
    )
  }
}
