-- Add migration script here
-- case files (core clinical record per admission episode)
CREATE TABLE case_files (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_number             VARCHAR(30) UNIQUE NOT NULL,
    patient_id              UUID NOT NULL REFERENCES patients(id) ON DELETE RESTRICT,
    primary_physician_id    UUID REFERENCES staff(id) ON DELETE SET NULL,
    department              VARCHAR(50) NOT NULL CHECK (
                                department IN (
                                    'maternity',
                                    'surgery',
                                    'consultation',
                                    'mental_health',
                                    'pharmacy',
                                    'general'
                                )
                            ),
    status                  VARCHAR(20) NOT NULL DEFAULT 'open' CHECK (
                                status IN ('open', 'closed', 'discharged')
                            ),
    admission_type          VARCHAR(20) CHECK (
                                admission_type IN ('inpatient', 'outpatient', 'emergency', 'day_case')
                            ),
    admitted_at             TIMESTAMPTZ,
    discharged_at           TIMESTAMPTZ,
    chief_complaint         TEXT,
    notes                   TEXT,
    opened_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    closed_at               TIMESTAMPTZ,
    opened_by               UUID REFERENCES staff(id) ON DELETE SET NULL,
    closed_by               UUID REFERENCES staff(id) ON DELETE SET NULL,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cases_patient        ON case_files(patient_id);
CREATE INDEX idx_cases_physician      ON case_files(primary_physician_id);
CREATE INDEX idx_cases_status         ON case_files(status);
CREATE INDEX idx_cases_department     ON case_files(department);
CREATE INDEX idx_cases_opened_at      ON case_files(opened_at DESC);
-- composite: physician's open cases (most common query in the physician dashboard)
CREATE INDEX idx_cases_physician_open ON case_files(primary_physician_id, status)
    WHERE status = 'open';

-- diagnoses attached to a case
CREATE TABLE diagnoses (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_file_id    UUID NOT NULL REFERENCES case_files(id) ON DELETE CASCADE,
    physician_id    UUID REFERENCES staff(id) ON DELETE SET NULL,
    icd10_code      VARCHAR(10) NOT NULL,
    description     TEXT NOT NULL,
    severity        VARCHAR(20) CHECK (
                        severity IN ('mild', 'moderate', 'severe', 'critical')
                    ),
    diagnosed_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes           TEXT
);

CREATE INDEX idx_diagnoses_case       ON diagnoses(case_file_id);
CREATE INDEX idx_diagnoses_icd10      ON diagnoses(icd10_code);

-- appointments
CREATE TABLE appointments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id          UUID NOT NULL REFERENCES patients(id) ON DELETE RESTRICT,
    physician_id        UUID REFERENCES staff(id) ON DELETE SET NULL,
    booked_by           UUID REFERENCES staff(id) ON DELETE SET NULL,
    department          VARCHAR(50) NOT NULL,
    appointment_type    VARCHAR(30) NOT NULL CHECK (
                            appointment_type IN (
                                'consultation',
                                'follow_up',
                                'procedure',
                                'surgery',
                                'prenatal',
                                'postnatal',
                                'mental_health',
                                'emergency'
                            )
                        ),
    status              VARCHAR(20) NOT NULL DEFAULT 'scheduled' CHECK (
                            status IN (
                                'scheduled',
                                'confirmed',
                                'completed',
                                'cancelled',
                                'no_show'
                            )
                        ),
    scheduled_at        TIMESTAMPTZ NOT NULL,
    duration_minutes    INTEGER NOT NULL DEFAULT 30,
    reason              TEXT,
    notes               TEXT,
    channel             VARCHAR(20) DEFAULT 'in_person' CHECK (
                            channel IN ('in_person', 'telehealth', 'phone')
                        ),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_appts_patient        ON appointments(patient_id);
CREATE INDEX idx_appts_physician      ON appointments(physician_id);
CREATE INDEX idx_appts_scheduled_at   ON appointments(scheduled_at DESC);
CREATE INDEX idx_appts_status         ON appointments(status);
-- composite: physician daily schedule (the desk appointment board query)
CREATE INDEX idx_appts_physician_date ON appointments(physician_id, scheduled_at)
    WHERE status NOT IN ('cancelled', 'no_show');

-- appointment reminders
CREATE TABLE appointment_reminders (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    appointment_id  UUID NOT NULL REFERENCES appointments(id) ON DELETE CASCADE,
    channel         VARCHAR(20) NOT NULL CHECK (channel IN ('sms', 'email', 'push')),
    status          VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (
                        status IN ('pending', 'sent', 'failed')
                    ),
    scheduled_for   TIMESTAMPTZ NOT NULL,
    sent_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_reminders_appointment ON appointment_reminders(appointment_id);
CREATE INDEX idx_reminders_scheduled   ON appointment_reminders(scheduled_for)
    WHERE status = 'pending';
