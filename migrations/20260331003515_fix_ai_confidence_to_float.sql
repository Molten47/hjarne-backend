ALTER TABLE prescriptions
    ALTER COLUMN ai_confidence TYPE FLOAT8 USING ai_confidence::FLOAT8;-- Add migration script here
