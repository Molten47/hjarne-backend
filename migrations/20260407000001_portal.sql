-- portal invites
CREATE TABLE portal_invites (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id   UUID        NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    token        UUID        NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    invited_by   UUID        NOT NULL REFERENCES staff(id),
    sent_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at   TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '72 hours',
    consumed_at  TIMESTAMPTZ,
    CONSTRAINT one_active_invite_per_patient
        UNIQUE NULLS NOT DISTINCT (patient_id, consumed_at)
);

CREATE INDEX idx_portal_invites_token      ON portal_invites(token);
CREATE INDEX idx_portal_invites_patient_id ON portal_invites(patient_id);

-- portal messages (patient <-> physician chat)
CREATE TABLE portal_messages (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id   UUID        NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    staff_id     UUID        NOT NULL REFERENCES staff(id),
    body         TEXT        NOT NULL,
    sender_type  TEXT        NOT NULL CHECK (sender_type IN ('patient', 'staff')),
    sent_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at      TIMESTAMPTZ
);

CREATE INDEX idx_portal_messages_patient_id ON portal_messages(patient_id);
CREATE INDEX idx_portal_messages_staff_id   ON portal_messages(staff_id);

-- complaints
CREATE TABLE portal_complaints (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    patient_id   UUID        NOT NULL REFERENCES patients(id) ON DELETE CASCADE,
    subject      TEXT        NOT NULL,
    body         TEXT        NOT NULL,
    status       TEXT        NOT NULL DEFAULT 'open'
                             CHECK (status IN ('open', 'reviewed', 'resolved')),
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at  TIMESTAMPTZ,
    resolved_by  UUID        REFERENCES staff(id)
);

CREATE INDEX idx_portal_complaints_patient_id ON portal_complaints(patient_id);

-- daily.co room url on appointments
ALTER TABLE appointments
    ADD COLUMN IF NOT EXISTS daily_room_url TEXT;