-- Create setting table
CREATE TABLE setting (
    id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    key VARCHAR(200) NOT NULL UNIQUE COLLATE NOCASE,
    value TEXT,
    type VARCHAR(20)
);

-- Create index on key for faster lookups
CREATE INDEX idx_setting_key ON setting(key);

-- Create index on type for getSettings queries
CREATE INDEX idx_setting_type ON setting(type);
