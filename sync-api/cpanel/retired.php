<?php
// Retired DNS endpoint. Never forward credentials or initialize the old writer.
http_response_code(410);
header('Content-Type: application/json');
header('Cache-Control: no-store');
header('X-Robots-Tag: noindex, nofollow');
if (($_SERVER['REQUEST_METHOD'] ?? 'GET') !== 'HEAD') {
    echo '{"error":"This endpoint has retired. Refresh DNS and retry the canonical API."}';
}
