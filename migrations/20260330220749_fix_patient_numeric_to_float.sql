ALTER TABLE patients
    ALTER COLUMN height_cm TYPE FLOAT8 USING height_cm::FLOAT8,
    ALTER COLUMN weight_kg TYPE FLOAT8 USING weight_kg::FLOAT8,
    ALTER COLUMN bmi       TYPE FLOAT8 USING bmi::FLOAT8;-- Add migration script here
