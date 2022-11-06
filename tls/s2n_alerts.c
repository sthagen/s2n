/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 *
 * Licensed under the Apache License, Version 2.0 (the "License").
 * You may not use this file except in compliance with the License.
 * A copy of the License is located at
 *
 *  http://aws.amazon.com/apache2.0
 *
 * or in the "license" file accompanying this file. This file is distributed
 * on an "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either
 * express or implied. See the License for the specific language governing
 * permissions and limitations under the License.
 */

#include <stdint.h>
#include <sys/param.h>

#include "error/s2n_errno.h"

#include "tls/s2n_tls_parameters.h"
#include "tls/s2n_connection.h"
#include "tls/s2n_record.h"
#include "tls/s2n_resume.h"
#include "tls/s2n_alerts.h"

#include "utils/s2n_safety.h"
#include "utils/s2n_blob.h"

#define S2N_TLS_ALERT_LEVEL_WARNING         1
#define S2N_TLS_ALERT_LEVEL_FATAL           2

#define S2N_ALERT_CASE(error, alert_code) \
    case (error): \
        *alert = (alert_code); \
        return S2N_RESULT_OK

#define S2N_NO_ALERT(error) \
    case (error): \
        RESULT_BAIL(S2N_ERR_NO_ALERT)

static S2N_RESULT s2n_translate_protocol_error_to_alert(int error_code, uint8_t *alert)
{
    RESULT_ENSURE_REF(alert);

    switch(error_code) {
        S2N_ALERT_CASE(S2N_ERR_MISSING_EXTENSION, S2N_TLS_ALERT_MISSING_EXTENSION);

        /* TODO: The ERR_BAD_MESSAGE -> ALERT_UNEXPECTED_MESSAGE mapping
         * isn't always correct. Sometimes s2n-tls uses ERR_BAD_MESSAGE
         * to indicate S2N_TLS_ALERT_ILLEGAL_PARAMETER instead.
         * We'll want to add a new error to distinguish between the two usages:
         * our errors should be equally or more specific than alerts, not less.
         */
        S2N_ALERT_CASE(S2N_ERR_BAD_MESSAGE, S2N_TLS_ALERT_UNEXPECTED_MESSAGE);

        /* For errors involving secure renegotiation:
         *= https://tools.ietf.org/rfc/rfc5746#3.4
         *# Note: later in Section 3, "abort the handshake" is used as
         *# shorthand for "send a fatal handshake_failure alert and
         *# terminate the connection".
         */
        S2N_ALERT_CASE(S2N_ERR_NO_RENEGOTIATION, S2N_TLS_ALERT_HANDSHAKE_FAILURE);

        /* TODO: Add mappings for other protocol errors.
         */
        S2N_NO_ALERT(S2N_ERR_ENCRYPT);
        S2N_NO_ALERT(S2N_ERR_DECRYPT);
        S2N_NO_ALERT(S2N_ERR_KEY_INIT);
        S2N_NO_ALERT(S2N_ERR_KEY_DESTROY);
        S2N_NO_ALERT(S2N_ERR_DH_SERIALIZING);
        S2N_NO_ALERT(S2N_ERR_DH_SHARED_SECRET);
        S2N_NO_ALERT(S2N_ERR_DH_WRITING_PUBLIC_KEY);
        S2N_NO_ALERT(S2N_ERR_DH_FAILED_SIGNING);
        S2N_NO_ALERT(S2N_ERR_DH_COPYING_PARAMETERS);
        S2N_NO_ALERT(S2N_ERR_DH_GENERATING_PARAMETERS);
        S2N_NO_ALERT(S2N_ERR_CIPHER_NOT_SUPPORTED);
        S2N_NO_ALERT(S2N_ERR_NO_APPLICATION_PROTOCOL);
        S2N_NO_ALERT(S2N_ERR_FALLBACK_DETECTED);
        S2N_NO_ALERT(S2N_ERR_HASH_DIGEST_FAILED);
        S2N_NO_ALERT(S2N_ERR_HASH_INIT_FAILED);
        S2N_NO_ALERT(S2N_ERR_HASH_UPDATE_FAILED);
        S2N_NO_ALERT(S2N_ERR_HASH_COPY_FAILED);
        S2N_NO_ALERT(S2N_ERR_HASH_WIPE_FAILED);
        S2N_NO_ALERT(S2N_ERR_HASH_NOT_READY);
        S2N_NO_ALERT(S2N_ERR_ALLOW_MD5_FOR_FIPS_FAILED);
        S2N_NO_ALERT(S2N_ERR_DECODE_CERTIFICATE);
        S2N_NO_ALERT(S2N_ERR_DECODE_PRIVATE_KEY);
        S2N_NO_ALERT(S2N_ERR_INVALID_HELLO_RETRY);
        S2N_NO_ALERT(S2N_ERR_INVALID_SIGNATURE_ALGORITHM);
        S2N_NO_ALERT(S2N_ERR_INVALID_SIGNATURE_SCHEME);
        S2N_NO_ALERT(S2N_ERR_CBC_VERIFY);
        S2N_NO_ALERT(S2N_ERR_DH_COPYING_PUBLIC_KEY);
        S2N_NO_ALERT(S2N_ERR_SIGN);
        S2N_NO_ALERT(S2N_ERR_VERIFY_SIGNATURE);
        S2N_NO_ALERT(S2N_ERR_ECDHE_GEN_KEY);
        S2N_NO_ALERT(S2N_ERR_ECDHE_SHARED_SECRET);
        S2N_NO_ALERT(S2N_ERR_ECDHE_UNSUPPORTED_CURVE);
        S2N_NO_ALERT(S2N_ERR_ECDSA_UNSUPPORTED_CURVE);
        S2N_NO_ALERT(S2N_ERR_ECDHE_SERIALIZING);
        S2N_NO_ALERT(S2N_ERR_KEM_UNSUPPORTED_PARAMS);
        S2N_NO_ALERT(S2N_ERR_SHUTDOWN_RECORD_TYPE);
        S2N_NO_ALERT(S2N_ERR_SHUTDOWN_CLOSED);
        S2N_NO_ALERT(S2N_ERR_NON_EMPTY_RENEGOTIATION_INFO);
        S2N_NO_ALERT(S2N_ERR_RECORD_LIMIT);
        S2N_NO_ALERT(S2N_ERR_CERT_UNTRUSTED);
        S2N_NO_ALERT(S2N_ERR_CERT_REVOKED);
        S2N_NO_ALERT(S2N_ERR_CERT_EXPIRED);
        S2N_NO_ALERT(S2N_ERR_CERT_TYPE_UNSUPPORTED);
        S2N_NO_ALERT(S2N_ERR_CERT_INVALID);
        S2N_NO_ALERT(S2N_ERR_CERT_MAX_CHAIN_DEPTH_EXCEEDED);
        S2N_NO_ALERT(S2N_ERR_CRL_LOOKUP_FAILED);
        S2N_NO_ALERT(S2N_ERR_CRL_SIGNATURE);
        S2N_NO_ALERT(S2N_ERR_CRL_ISSUER);
        S2N_NO_ALERT(S2N_ERR_CRL_UNHANDLED_CRITICAL_EXTENSION);
        S2N_NO_ALERT(S2N_ERR_INVALID_MAX_FRAG_LEN);
        S2N_NO_ALERT(S2N_ERR_MAX_FRAG_LEN_MISMATCH);
        S2N_NO_ALERT(S2N_ERR_PROTOCOL_VERSION_UNSUPPORTED);
        S2N_NO_ALERT(S2N_ERR_BAD_KEY_SHARE);
        S2N_NO_ALERT(S2N_ERR_CANCELLED);
        S2N_NO_ALERT(S2N_ERR_PROTOCOL_DOWNGRADE_DETECTED);
        S2N_NO_ALERT(S2N_ERR_MAX_INNER_PLAINTEXT_SIZE);
        S2N_NO_ALERT(S2N_ERR_RECORD_STUFFER_SIZE);
        S2N_NO_ALERT(S2N_ERR_FRAGMENT_LENGTH_TOO_LARGE);
        S2N_NO_ALERT(S2N_ERR_FRAGMENT_LENGTH_TOO_SMALL);
        S2N_NO_ALERT(S2N_ERR_RECORD_STUFFER_NEEDS_DRAINING);
        S2N_NO_ALERT(S2N_ERR_UNSUPPORTED_EXTENSION);
        S2N_NO_ALERT(S2N_ERR_DUPLICATE_EXTENSION);
        S2N_NO_ALERT(S2N_ERR_MAX_EARLY_DATA_SIZE);
        S2N_NO_ALERT(S2N_ERR_EARLY_DATA_TRIAL_DECRYPT);
    }

    RESULT_BAIL(S2N_ERR_UNIMPLEMENTED);
}

static bool s2n_alerts_supported(struct s2n_connection *conn)
{
    /* If running in QUIC mode, QUIC handles alerting.
     * S2N should not send or receive alerts. */
    return !s2n_connection_is_quic_enabled(conn);
}

static bool s2n_process_as_warning(struct s2n_connection *conn, uint8_t level, uint8_t type)
{
    /* Only TLS1.2 considers the alert level. The alert level field is
     * considered deprecated in TLS1.3. */
    if (s2n_connection_get_protocol_version(conn) < S2N_TLS13) {
        return level == S2N_TLS_ALERT_LEVEL_WARNING
                && conn->config->alert_behavior == S2N_ALERT_IGNORE_WARNINGS;
    }

    /* user_canceled is the only alert currently treated as a warning in TLS1.3.
     * We need to treat it as a warning regardless of alert_behavior to avoid marking
     * correctly-closed connections as failed. */
    return type == S2N_TLS_ALERT_USER_CANCELED;
}

S2N_RESULT s2n_alerts_close_if_fatal(struct s2n_connection *conn, struct s2n_blob *alert)
{
    RESULT_ENSURE_REF(conn);
    RESULT_ENSURE_REF(alert);
    RESULT_ENSURE_EQ(alert->size, S2N_ALERT_LENGTH);
    /* Only one alert should currently be treated as a warning */
    if (alert->data[1] == S2N_TLS_ALERT_NO_RENEGOTIATION) {
        RESULT_ENSURE_EQ(alert->data[0], S2N_TLS_ALERT_LEVEL_WARNING);
        return S2N_RESULT_OK;
    }
    conn->closing = true;
    return S2N_RESULT_OK;
}

int s2n_error_get_alert(int error, uint8_t *alert)
{
    int error_type = s2n_error_get_type(error);

    POSIX_ENSURE_REF(alert);

    switch(error_type) {
        case S2N_ERR_T_OK:
        case S2N_ERR_T_CLOSED:
        case S2N_ERR_T_BLOCKED:
        case S2N_ERR_T_USAGE:
        case S2N_ERR_T_ALERT:
            POSIX_BAIL(S2N_ERR_NO_ALERT);
            break;
        case S2N_ERR_T_PROTO:
            POSIX_GUARD_RESULT(s2n_translate_protocol_error_to_alert(error, alert));
            break;
        case S2N_ERR_T_IO:
        case S2N_ERR_T_INTERNAL:
            *alert = S2N_TLS_ALERT_INTERNAL_ERROR;
            break;
    }

    return S2N_SUCCESS;
}

int s2n_process_alert_fragment(struct s2n_connection *conn)
{
    POSIX_ENSURE_REF(conn);
    S2N_ERROR_IF(s2n_stuffer_data_available(&conn->in) == 0, S2N_ERR_BAD_MESSAGE);
    S2N_ERROR_IF(s2n_stuffer_data_available(&conn->alert_in) == 2, S2N_ERR_ALERT_PRESENT);
    POSIX_ENSURE(s2n_alerts_supported(conn), S2N_ERR_BAD_MESSAGE);

    while (s2n_stuffer_data_available(&conn->in)) {
        uint8_t bytes_required = 2;

        /* Alerts are two bytes long, but can still be fragmented or coalesced */
        if (s2n_stuffer_data_available(&conn->alert_in) == 1) {
            bytes_required = 1;
        }

        int bytes_to_read = MIN(bytes_required, s2n_stuffer_data_available(&conn->in));

        POSIX_GUARD(s2n_stuffer_copy(&conn->in, &conn->alert_in, bytes_to_read));

        if (s2n_stuffer_data_available(&conn->alert_in) == 2) {

            /* Close notifications are handled as shutdowns */
            if (conn->alert_in_data[1] == S2N_TLS_ALERT_CLOSE_NOTIFY) {
                conn->closed = 1;
                conn->close_notify_received = true;
                return 0;
            }

            /* Ignore warning-level alerts if we're in warning-tolerant mode */
            if (s2n_process_as_warning(conn, conn->alert_in_data[0], conn->alert_in_data[1])) {
                POSIX_GUARD(s2n_stuffer_wipe(&conn->alert_in));
                return 0;
            }

            /* RFC 5077 5.1 - Expire any cached session on an error alert */
            if (s2n_allowed_to_cache_connection(conn) && conn->session_id_len) {
                conn->config->cache_delete(conn, conn->config->cache_delete_data, conn->session_id, conn->session_id_len);
            }

            /* All other alerts are treated as fatal errors */
            conn->closed = 1;
            POSIX_BAIL(S2N_ERR_ALERT);
        }
    }

    return 0;
}

int s2n_queue_writer_close_alert_warning(struct s2n_connection *conn)
{
    POSIX_ENSURE_REF(conn);

    uint8_t alert[2];
    alert[0] = S2N_TLS_ALERT_LEVEL_WARNING;
    alert[1] = S2N_TLS_ALERT_CLOSE_NOTIFY;

    struct s2n_blob out = {.data = alert,.size = sizeof(alert) };

    /* If there is an alert pending or we've already sent a close_notify, do nothing */
    if (s2n_stuffer_data_available(&conn->writer_alert_out) || conn->close_notify_queued) {
        return S2N_SUCCESS;
    }

    if (!s2n_alerts_supported(conn)) {
        return S2N_SUCCESS;
    }

    POSIX_GUARD(s2n_stuffer_write(&conn->writer_alert_out, &out));
    conn->close_notify_queued = 1;

    return S2N_SUCCESS;
}

static int s2n_queue_reader_alert(struct s2n_connection *conn, uint8_t level, uint8_t error_code)
{
    POSIX_ENSURE_REF(conn);

    uint8_t alert[2];
    alert[0] = level;
    alert[1] = error_code;

    struct s2n_blob out = {.data = alert,.size = sizeof(alert) };

    /* If there is an alert pending, do nothing */
    if (s2n_stuffer_data_available(&conn->reader_alert_out)) {
        return S2N_SUCCESS;
    }

    if (!s2n_alerts_supported(conn)) {
        return S2N_SUCCESS;
    }

    POSIX_GUARD(s2n_stuffer_write(&conn->reader_alert_out, &out));

    return S2N_SUCCESS;
}

int s2n_queue_reader_unsupported_protocol_version_alert(struct s2n_connection *conn)
{
    return s2n_queue_reader_alert(conn, S2N_TLS_ALERT_LEVEL_FATAL, S2N_TLS_ALERT_PROTOCOL_VERSION);
}

int s2n_queue_reader_handshake_failure_alert(struct s2n_connection *conn)
{
    return s2n_queue_reader_alert(conn, S2N_TLS_ALERT_LEVEL_FATAL, S2N_TLS_ALERT_HANDSHAKE_FAILURE);
}

S2N_RESULT s2n_queue_reader_no_renegotiation_alert(struct s2n_connection *conn)
{
    /**
     *= https://tools.ietf.org/rfc/rfc5746#4.5
     *# SSLv3 does not define the "no_renegotiation" alert (and does
     *# not offer a way to indicate a refusal to renegotiate at a "warning"
     *# level).  SSLv3 clients that refuse renegotiation SHOULD use a fatal
     *# handshake_failure alert.
     **/
    if (s2n_connection_get_protocol_version(conn) == S2N_SSLv3) {
        RESULT_GUARD_POSIX(s2n_queue_reader_alert(conn, S2N_TLS_ALERT_LEVEL_FATAL, S2N_TLS_ALERT_HANDSHAKE_FAILURE));
        return S2N_RESULT_OK;
    }

    RESULT_GUARD_POSIX(s2n_queue_reader_alert(conn, S2N_TLS_ALERT_LEVEL_WARNING, S2N_TLS_ALERT_NO_RENEGOTIATION));
    return S2N_RESULT_OK;
}
