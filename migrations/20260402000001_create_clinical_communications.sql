-- clinical communications between staff, scoped to a case
-- supports typed notes, structured handoff forms, and file attachments
CREATE TABLE clinical_communications (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_file_id    UUID NOT NULL REFERENCES case_files(id) ON DELETE RESTRICT,
    sender_id       UUID NOT NULL REFERENCES auth_users(id) ON DELETE RESTRICT,
    recipient_id    UUID REFERENCES auth_users(id) ON DELETE RESTRICT,
    -- NULL recipient = broadcast to all staff on the case
    comm_type       VARCHAR(20) NOT NULL DEFAULT 'note' CHECK (
                        comm_type IN ('note', 'handoff', 'upload')
                    ),
    subject         VARCHAR(255) NOT NULL,
    body            TEXT NOT NULL DEFAULT '',
    status          VARCHAR(20) NOT NULL DEFAULT 'sent' CHECK (
                        status IN ('sent', 'read', 'acknowledged')
                    ),
    read_at         TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_clcomms_case       ON clinical_communications(case_file_id);
CREATE INDEX idx_clcomms_sender     ON clinical_communications(sender_id);
CREATE INDEX idx_clcomms_recipient  ON clinical_communications(recipient_id);
CREATE INDEX idx_clcomms_created    ON clinical_communications(created_at DESC);
-- unread count per recipient per case
CREATE INDEX idx_clcomms_unread     ON clinical_communications(recipient_id, case_file_id)
    WHERE status = 'sent';

-- file attachments — bytea storage, one comm can have multiple files
CREATE TABLE clinical_attachments (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    communication_id    UUID NOT NULL REFERENCES clinical_communications(id) ON DELETE CASCADE,
    file_name           VARCHAR(255) NOT NULL,
    file_type           VARCHAR(100) NOT NULL,
    file_size           INTEGER NOT NULL,
    file_data           BYTEA NOT NULL,
    uploaded_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_clattach_comm      ON clinical_attachments(communication_id);