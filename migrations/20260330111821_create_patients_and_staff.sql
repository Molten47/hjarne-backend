-- patients core table
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE patients (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    mrn         VARCHAR(20) UNIQUE NOT NULL,
    first_name  VARCHAR(100) NOT NULL,
    last_name   VARCHAR(100) NOT NULL,
    date_of_birth DATE NOT NULL,
    gender      VARCHAR(20) NOT NULL,
    blood_group VARCHAR(10),
    genotype    VARCHAR(10),
    height_cm   DECIMAL(5,2),
    weight_kg   DECIMAL(5,2),
    bmi         DECIMAL(4,2),
    nationality VARCHAR(60),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_patients_mrn ON patients(mrn);
CREATE INDEX idx_patients_name ON patients(last_name, first_name);
CREATE INDEX idx_patients_dob  ON patients(date_of_birth);

-- staff table
CREATE TABLE staff (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    staff_code      VARCHAR(20) UNIQUE NOT NULL,
    first_name      VARCHAR(100) NOT NULL,
    last_name       VARCHAR(100) NOT NULL,
    role            VARCHAR(30) NOT NULL,
    department      VARCHAR(50),
    specialization  VARCHAR(100),
    license_number  VARCHAR(50),
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_staff_role       ON staff(role);
CREATE INDEX idx_staff_department ON staff(department);
CREATE INDEX idx_staff_code       ON staff(staff_code);