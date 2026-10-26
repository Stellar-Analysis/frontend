-- Hash-chained, tamper-evident administrative audit log schema and role privileges.
--
-- Separation of privileges:
--   * stellar_app_writer     : general application write role (NO INSERT/UPDATE/DELETE on admin_audit_log)
--   * stellar_audit_appender : append-only audit role (INSERT, SELECT only)
--   * stellar_audit_verifier : incident-response verifier role (SELECT only)

CREATE TABLE IF NOT EXISTS admin_audit_log (
    sequence     BIGINT PRIMARY KEY CHECK (sequence > 0),
    prev_hash    CHAR(64) NOT NULL,
    action       TEXT NOT NULL,
    actor        TEXT NOT NULL,
    timestamp_ms BIGINT NOT NULL CHECK (timestamp_ms >= 0),
    entry_hash   CHAR(64) NOT NULL UNIQUE
);

DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'stellar_app_writer') THEN
        CREATE ROLE stellar_app_writer NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'stellar_audit_appender') THEN
        CREATE ROLE stellar_audit_appender NOLOGIN;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'stellar_audit_verifier') THEN
        CREATE ROLE stellar_audit_verifier NOLOGIN;
    END IF;
END
$$;

-- Revoke all mutating privileges from PUBLIC and ordinary application roles.
REVOKE ALL ON TABLE admin_audit_log FROM PUBLIC;
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON TABLE admin_audit_log FROM stellar_app_writer;
REVOKE UPDATE, DELETE, TRUNCATE ON TABLE admin_audit_log FROM stellar_audit_appender;
REVOKE INSERT, UPDATE, DELETE, TRUNCATE ON TABLE admin_audit_log FROM stellar_audit_verifier;

-- Grant strictly scoped append-only and read-only access.
GRANT SELECT ON TABLE admin_audit_log TO stellar_app_writer;
GRANT SELECT, INSERT ON TABLE admin_audit_log TO stellar_audit_appender;
GRANT SELECT ON TABLE admin_audit_log TO stellar_audit_verifier;

-- Defense-in-depth trigger blocking UPDATE and DELETE even if privileges are misconfigured.
CREATE OR REPLACE FUNCTION prevent_admin_audit_log_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'admin_audit_log is append-only; % operations are prohibited', TG_OP;
END;
$$;

DROP TRIGGER IF EXISTS trg_admin_audit_log_immutable ON admin_audit_log;
CREATE TRIGGER trg_admin_audit_log_immutable
BEFORE UPDATE OR DELETE OR TRUNCATE ON admin_audit_log
FOR EACH STATEMENT
EXECUTE FUNCTION prevent_admin_audit_log_mutation();
