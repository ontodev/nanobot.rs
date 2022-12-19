CREATE INDEX IF NOT EXISTS message_level_idx ON message(level);
CREATE INDEX IF NOT EXISTS message_table_row_column_idx ON message("table", row, column);
CREATE INDEX IF NOT EXISTS message_table_row_idx ON message("table", row);

DROP TABLE IF EXISTS levels;
CREATE TABLE levels (
  level TEXT,
  severity INT
);
INSERT INTO levels VALUES
('error', 4),
('warn', 3),
('info', 2),
('update', 1);

DROP VIEW IF EXISTS message_cell;
CREATE VIEW message_cell AS
SELECT "table",
  "row",
  "column",
  "value",
  json_group_array(
    json_object(
      'level', message."level",
      'agent', "agent",
      'rule', "rule",
      'message', "message"
    )
  ) AS messages,
  SUM(message.level IS NOT NULL) AS message_count,
  SUM(message.level = 'error') AS error_count,
  SUM(message.level = 'warn') AS warn_count,
  SUM(message.level = 'info') AS info_count,
  SUM(message.level = 'update') AS update_count,
  MAX(severity) AS severity
FROM message
JOIN levels ON message.level = levels.level
GROUP BY "table", "row", "column";

