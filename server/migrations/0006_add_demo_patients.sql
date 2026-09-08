BEGIN;

WITH jane AS (
    INSERT INTO patients (legal_name, birth_date, sex)
    SELECT 'Jane Doe', DATE '1978-03-15', 'female'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Jane Doe')
    RETURNING id
), jane_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00018452', 'MRN', TRUE FROM jane
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555010203', TRUE FROM jane;

WITH john AS (
    INSERT INTO patients (legal_name, birth_date, sex)
    SELECT 'John Smith', DATE '1965-11-02', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'John Smith')
    RETURNING id
), john_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00021190', 'MRN', TRUE FROM john
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555040506', TRUE FROM john;

WITH samir AS (
    INSERT INTO patients (legal_name, preferred_name, birth_date, sex)
    SELECT 'Samir Amrani', 'Sam Amrani', DATE '1990-06-21', 'male'
    WHERE NOT EXISTS (SELECT 1 FROM patients WHERE legal_name = 'Samir Amrani')
    RETURNING id
), samir_mrn AS (
    INSERT INTO patient_identifiers (patient_id, system, value, identifier_type, is_primary)
    SELECT id, 'thyme-mrn', '00033761', 'MRN', TRUE FROM samir
)
INSERT INTO patient_contacts (patient_id, type, value, preferred)
SELECT id, 'phone', '+213555070809', TRUE FROM samir;

COMMIT;
