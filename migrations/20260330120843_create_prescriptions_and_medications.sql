-- drug reference table
-- this is the master list of all drugs in the system
CREATE TABLE drugs (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                VARCHAR(150) UNIQUE NOT NULL,
    generic_name        VARCHAR(150),
    category            VARCHAR(80),
    drug_class          VARCHAR(80),
    contraindications   TEXT[],
    interactions        TEXT[],
    unit                VARCHAR(30),
    is_controlled       BOOLEAN NOT NULL DEFAULT FALSE,
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_drugs_name           ON drugs(name);
CREATE INDEX idx_drugs_generic        ON drugs(generic_name);
CREATE INDEX idx_drugs_category       ON drugs(category);
CREATE INDEX idx_drugs_active         ON drugs(is_active) WHERE is_active = TRUE;

-- seed some common North American drugs for dev/demo
INSERT INTO drugs (name, generic_name, category, drug_class, contraindications, interactions, unit, is_controlled) VALUES
('Tylenol',       'Acetaminophen',  'Analgesic',     'Para-aminophenol',   ARRAY['hepatic impairment', 'alcohol use disorder'],                   ARRAY['warfarin', 'alcohol'],          'mg',  FALSE),
('Advil',         'Ibuprofen',      'NSAID',          'Propionic acid',     ARRAY['peptic ulcer', 'renal impairment', 'third trimester pregnancy'], ARRAY['aspirin', 'warfarin', 'lithium'],'mg',  FALSE),
('Amoxicillin',   'Amoxicillin',    'Antibiotic',     'Penicillin',         ARRAY['penicillin allergy'],                                           ARRAY['warfarin', 'methotrexate'],     'mg',  FALSE),
('Lisinopril',    'Lisinopril',     'Antihypertensive','ACE Inhibitor',     ARRAY['pregnancy', 'angioedema history', 'bilateral renal artery stenosis'], ARRAY['potassium', 'NSAIDs', 'lithium'], 'mg', FALSE),
('Metformin',     'Metformin',      'Antidiabetic',   'Biguanide',          ARRAY['renal impairment', 'hepatic impairment', 'contrast dye procedures'], ARRAY['alcohol', 'contrast agents'], 'mg', FALSE),
('Atorvastatin',  'Atorvastatin',   'Statin',         'HMG-CoA reductase',  ARRAY['active liver disease', 'pregnancy'],                            ARRAY['cyclosporine', 'clarithromycin', 'niacin'], 'mg', FALSE),
('Azithromycin',  'Azithromycin',   'Antibiotic',     'Macrolide',          ARRAY['macrolide allergy', 'hepatic impairment'],                      ARRAY['warfarin', 'digoxin', 'antacids'], 'mg', FALSE),
('Hydrocodone',   'Hydrocodone',    'Opioid Analgesic','Opioid',            ARRAY['respiratory depression', 'MAO inhibitor use'],                  ARRAY['benzodiazepines', 'alcohol', 'CNS depressants'], 'mg', TRUE),
('Lorazepam',     'Lorazepam',      'Anxiolytic',     'Benzodiazepine',     ARRAY['acute narrow-angle glaucoma', 'respiratory depression'],        ARRAY['opioids', 'alcohol', 'CNS depressants'], 'mg', TRUE),
('Prednisone',    'Prednisone',     'Corticosteroid', 'Glucocorticoid',     ARRAY['systemic fungal infection', 'live vaccines'],                   ARRAY['NSAIDs', 'warfarin', 'antidiabetics'], 'mg', FALSE),
('Prenatal DHA',  'DHA/Folic Acid', 'Supplement',  'Prenatal Vitamin', ARRAY[]::TEXT[], ARRAY[]::TEXT[],          'mg',    FALSE),
('Oxytocin',      'Oxytocin',       'Uterotonic',  'Hormone',          ARRAY['fetal distress', 'abnormal fetal position'], ARRAY['prostaglandins'], 'units', FALSE);
-- prescriptions (the order a physician writes)
CREATE TABLE prescriptions (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_file_id        UUID NOT NULL REFERENCES case_files(id) ON DELETE RESTRICT,
    prescribed_by       UUID NOT NULL REFERENCES staff(id) ON DELETE RESTRICT,
    status              VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (
                            status IN ('pending', 'approved', 'dispensed', 'cancelled')
                        ),
    prescribed_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_until         TIMESTAMPTZ,
    ai_recommendation   TEXT,
    ai_confidence       DECIMAL(4,3),
    physician_approved  BOOLEAN NOT NULL DEFAULT FALSE,
    approved_at         TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rx_case_file         ON prescriptions(case_file_id);
CREATE INDEX idx_rx_prescribed_by     ON prescriptions(prescribed_by);
CREATE INDEX idx_rx_status            ON prescriptions(status);
-- pharmacy queue index -- pending approved prescriptions only
CREATE INDEX idx_rx_pharmacy_queue    ON prescriptions(status, prescribed_at DESC)
    WHERE status = 'approved' AND physician_approved = TRUE;

-- individual drug line items within a prescription
CREATE TABLE prescription_items (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    prescription_id             UUID NOT NULL REFERENCES prescriptions(id) ON DELETE CASCADE,
    drug_id                     UUID NOT NULL REFERENCES drugs(id) ON DELETE RESTRICT,
    dosage                      VARCHAR(50) NOT NULL,
    frequency                   VARCHAR(80) NOT NULL,
    route                       VARCHAR(30) NOT NULL CHECK (
                                    route IN (
                                        'oral', 'iv', 'im', 'subcutaneous',
                                        'topical', 'inhaled', 'sublingual', 'rectal'
                                    )
                                ),
    duration_days               INTEGER,
    instructions                TEXT,
    contraindication_flagged    BOOLEAN NOT NULL DEFAULT FALSE,
    contraindication_notes      TEXT,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_rx_items_prescription ON prescription_items(prescription_id);
CREATE INDEX idx_rx_items_drug         ON prescription_items(drug_id);
-- flag index -- quick query for items with contraindication warnings
CREATE INDEX idx_rx_items_flagged      ON prescription_items(contraindication_flagged)
    WHERE contraindication_flagged = TRUE;

-- medication administration record (MAR)
-- every time a nurse gives a drug this is recorded
CREATE TABLE medication_administered (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_file_id            UUID NOT NULL REFERENCES case_files(id) ON DELETE RESTRICT,
    prescription_item_id    UUID REFERENCES prescription_items(id) ON DELETE SET NULL,
    administered_by         UUID NOT NULL REFERENCES staff(id) ON DELETE RESTRICT,
    administered_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    dosage_given            VARCHAR(50) NOT NULL,
    route                   VARCHAR(30) NOT NULL,
    notes                   TEXT,
    created_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_mar_case_file        ON medication_administered(case_file_id);
CREATE INDEX idx_mar_administered_by  ON medication_administered(administered_by);
CREATE INDEX idx_mar_administered_at  ON medication_administered(administered_at DESC);

-- dispensary log (pharmacist records what was physically dispensed)
CREATE TABLE dispensary_log (
    id                      UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    drug_id                 UUID NOT NULL REFERENCES drugs(id) ON DELETE RESTRICT,
    prescription_item_id    UUID REFERENCES prescription_items(id) ON DELETE SET NULL,
    dispensed_by            UUID NOT NULL REFERENCES staff(id) ON DELETE RESTRICT,
    quantity                INTEGER NOT NULL,
    batch_number            VARCHAR(60),
    dispensed_at            TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    notes                   TEXT
);

CREATE INDEX idx_dispensary_drug      ON dispensary_log(drug_id);
CREATE INDEX idx_dispensary_by        ON dispensary_log(dispensed_by);
CREATE INDEX idx_dispensary_at        ON dispensary_log(dispensed_at DESC);