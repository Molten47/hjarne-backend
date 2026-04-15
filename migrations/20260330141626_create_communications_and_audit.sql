-- Add migration script here
-- lab results ordered by physicians
CREATE TABLE lab_results (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_file_id    UUID NOT NULL REFERENCES case_files(id) ON DELETE RESTRICT,
    ordered_by      UUID NOT NULL REFERENCES staff(id) ON DELETE RESTRICT,
    test_name       VARCHAR(150) NOT NULL,
    result_value    VARCHAR(150),
    unit            VARCHAR(30),
    reference_range VARCHAR(80),
    status          VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (
                        status IN ('pending', 'collected', 'processing', 'resulted', 'cancelled')
                    ),
    collected_at    TIMESTAMPTZ,
    resulted_at     TIMESTAMPTZ,
    storage_url     TEXT,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_labs_case_file    ON lab_results(case_file_id);
CREATE INDEX idx_labs_ordered_by   ON lab_results(ordered_by);
CREATE INDEX idx_labs_status       ON lab_results(status);
CREATE INDEX idx_labs_resulted_at  ON lab_results(resulted_at DESC)
    WHERE status = 'resulted';

-- physician to patient communications (secure messaging)
CREATE TABLE communications (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sender_id               UUID NOT NULL REFERENCES staff(id) ON DELETE RESTRICT,
    recipient_patient_id    UUID NOT NULL REFERENCES patients(id) ON DELETE RESTRICT,
    type                    VARCHAR(20) NOT NULL DEFAULT 'message' CHECK (
                                type IN ('message', 'result_notification', 'appointment_reminder', 'discharge_summary')
                            ),
    subject                 VARCHAR(255) NOT NULL,
    body                    TEXT NOT NULL,
    status                  VARCHAR(20) NOT NULL DEFAULT 'sent' CHECK (
                                status IN ('draft', 'sent', 'delivered', 'read')
                            ),
    sent_at                 TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    read_at                 TIMESTAMPTZ,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_comms_sender        ON communications(sender_id);
CREATE INDEX idx_comms_recipient     ON communications(recipient_patient_id);
CREATE INDEX idx_comms_status        ON communications(status);
CREATE INDEX idx_comms_sent_at       ON communications(sent_at DESC);
-- unread messages index for patient portal badge count
CREATE INDEX idx_comms_unread        ON communications(recipient_patient_id, status)
    WHERE status IN ('sent', 'delivered');

-- system notifications (WebSocket event persistence)
CREATE TABLE notifications (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    recipient_id    UUID NOT NULL REFERENCES auth_users(id) ON DELETE CASCADE,
    channel         VARCHAR(20) NOT NULL CHECK (
                        channel IN ('websocket', 'email', 'sms', 'push')
                    ),
    event_type      VARCHAR(50) NOT NULL,
    payload         JSONB NOT NULL DEFAULT '{}',
    status          VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (
                        status IN ('pending', 'delivered', 'failed', 'read')
                    ),
    retry_count     INTEGER NOT NULL DEFAULT 0,
    delivered_at    TIMESTAMPTZ,
    read_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_notif_recipient     ON notifications(recipient_id);
CREATE INDEX idx_notif_event_type    ON notifications(event_type);
CREATE INDEX idx_notif_status        ON notifications(status);
-- unread notification count index (bell icon query)
CREATE INDEX idx_notif_unread        ON notifications(recipient_id, created_at DESC)
    WHERE status IN ('pending', 'delivered');

-- staff shift schedules
CREATE TABLE staff_schedules (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    staff_id        UUID NOT NULL REFERENCES staff(id) ON DELETE CASCADE,
    work_date       DATE NOT NULL,
    shift_start     TIME NOT NULL,
    shift_end       TIME NOT NULL,
    department      VARCHAR(50),
    is_on_call      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_staff_date UNIQUE (staff_id, work_date)
);

CREATE INDEX idx_schedules_staff     ON staff_schedules(staff_id);
CREATE INDEX idx_schedules_date      ON staff_schedules(work_date);
CREATE INDEX idx_schedules_dept      ON staff_schedules(department, work_date);

-- audit log (immutable record of every action in the system)
CREATE TABLE audit_log (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id        UUID REFERENCES auth_users(id) ON DELETE SET NULL,
    actor_type      VARCHAR(20),
    action          VARCHAR(50) NOT NULL,
    entity_type     VARCHAR(50) NOT NULL,
    entity_id       UUID,
    before_state    JSONB,
    after_state     JSONB,
    ip_address      VARCHAR(45),
    user_agent      TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- audit log is append-only, never updated, never deleted
-- indexes optimized for admin queries by actor, entity, and time range
CREATE INDEX idx_audit_actor         ON audit_log(actor_id);
CREATE INDEX idx_audit_entity        ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_action        ON audit_log(action);
CREATE INDEX idx_audit_created_at    ON audit_log(created_at DESC);

-- drug stock levels (pharmacy inventory)
CREATE TABLE drug_stock (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    drug_id             UUID NOT NULL REFERENCES drugs(id) ON DELETE RESTRICT,
    quantity_on_hand    INTEGER NOT NULL DEFAULT 0,
    reorder_threshold   INTEGER NOT NULL DEFAULT 50,
    unit                VARCHAR(30),
    last_restocked_at   TIMESTAMPTZ,
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_drug_stock UNIQUE (drug_id)
);

CREATE INDEX idx_stock_drug          ON drug_stock(drug_id);
-- low stock query index (pharmacist dashboard alert)
CREATE INDEX idx_stock_low           ON drug_stock(quantity_on_hand)
    WHERE quantity_on_hand <= reorder_threshold;

-- seed initial stock levels for our demo drugs
INSERT INTO drug_stock (drug_id, quantity_on_hand, reorder_threshold, unit)
SELECT id, 
    CASE 
        WHEN is_controlled = TRUE THEN 50
        ELSE 200
    END,
    CASE
        WHEN is_controlled = TRUE THEN 20
        ELSE 50
    END,
    unit
FROM drugs
WHERE is_active = TRUE;