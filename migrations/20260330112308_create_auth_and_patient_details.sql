-- Add migration script here
-- auth users (polymorphic -- serves both staff and patients)
CREATE TABLE auth_users (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    entity_id       UUID NOT NULL,
    entity_type     VARCHAR(20) NOT NULL CHECK (entity_type IN ('staff', 'patient')),
    email           VARCHAR(255) UNIQUE NOT NULL,
    password_hash   VARCHAR(255) NOT NULL,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    last_login      TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_entity UNIQUE (entity_id, entity_type)
);

CREATE INDEX idx_auth_email       ON auth_users(email);
CREATE INDEX idx_auth_entity      ON auth_users(entity_id, entity_type);

-- refresh tokens
CREATE TABLE refresh_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    token_hash  VARCHAR(255) UNIQUE NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    ip_address  VARCHAR(45),
    user_agent  TEXT,
    revoked     BOOLEAN NOT NULL DEFAULT FALSE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_refresh_user_id  ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_token    ON refresh_tokens(token_hash);

-- patient contacts and next of kin
CREATE TABLE patient_contacts (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id          UUID NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    phone               VARCHAR(20),
    email               VARCHAR(255),
    address_line1       VARCHAR(150),
    address_line2       VARCHAR(150),
    city                VARCHAR(80),
    state_province      VARCHAR(80),
    zip_postal          VARCHAR(20),
    country             VARCHAR(60) NOT NULL DEFAULT 'United States',
    next_of_kin_name    VARCHAR(150),
    next_of_kin_phone   VARCHAR(20),
    next_of_kin_relation VARCHAR(50),
    emergency_contact   VARCHAR(150)
);

CREATE INDEX idx_contacts_patient ON patient_contacts(patient_id);

-- patient insurance
-- supports Medicare, Medicaid, Blue Cross, Aetna, OHIP, provincial Canadian plans etc.
CREATE TABLE patient_insurance (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id      UUID NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    insurance_type  VARCHAR(30) NOT NULL CHECK (
                        insurance_type IN (
                            'medicare',
                            'medicaid',
                            'private',
                            'ohip',
                            'provincial_canada',
                            'child_care',
                            'family_care',
                            'uninsured'
                        )
                    ),
    provider_name   VARCHAR(100),
    policy_number   VARCHAR(60),
    group_number    VARCHAR(60),
    subscriber_name VARCHAR(150),
    valid_from      DATE,
    valid_until     DATE,
    is_primary      BOOLEAN NOT NULL DEFAULT FALSE,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_insurance_patient    ON patient_insurance(patient_id);
CREATE INDEX idx_insurance_policy     ON patient_insurance(policy_number);
CREATE INDEX idx_insurance_active     ON patient_insurance(patient_id, is_active);