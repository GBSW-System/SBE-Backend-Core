SELECT s.user_id, EXISTS (
    SELECT 1
    FROM permissions p
             LEFT JOIN user_permissions up
                       ON up.permission_id = p.id AND up.user_id = s.user_id
                           AND up.assigned_at <= statement_timestamp()
                           AND (up.expires_at IS NULL OR up.expires_at > statement_timestamp())
    WHERE p.service_id = $2 AND p.is_active
      AND p.code = ('service.' || p.service_id::text || '.configuration.write')
      AND COALESCE(up.effect = 'ALLOW', EXISTS (
        SELECT 1
        FROM user_roles ur
                 JOIN roles r ON r.id = ur.role_id AND r.is_active
                 JOIN role_permissions rp ON rp.role_id = r.id
        WHERE ur.user_id = s.user_id AND rp.permission_id = p.id
          AND ur.assigned_at <= statement_timestamp()
          AND (ur.expires_at IS NULL OR ur.expires_at > statement_timestamp())
    ))
) AS can_write
FROM user_sessions s
         JOIN users u ON u.id = s.user_id AND u.status = 'ACTIVE'
WHERE s.token_hash = $1
  AND s.revoked_at IS NULL
  AND s.expires_at > statement_timestamp()