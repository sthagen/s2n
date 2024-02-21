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

#pragma once

#include <stdbool.h>
#include <stdio.h>
#include <string.h>

#include "api/s2n.h"
#include "utils/s2n_ensure.h"

/*
 * To easily retrieve error types, we split error values into two parts.
 * The upper 6 bits describe the error type and the lower bits describe the value within the category.
 * [ Error Type Bits(31-26) ][ Value Bits(25-0) ]
 */
#define S2N_ERR_NUM_VALUE_BITS 26

/* Start value for each error type. */
#define S2N_ERR_T_OK_START       (S2N_ERR_T_OK << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_IO_START       (S2N_ERR_T_IO << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_CLOSED_START   (S2N_ERR_T_CLOSED << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_BLOCKED_START  (S2N_ERR_T_BLOCKED << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_ALERT_START    (S2N_ERR_T_ALERT << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_PROTO_START    (S2N_ERR_T_PROTO << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_INTERNAL_START (S2N_ERR_T_INTERNAL << S2N_ERR_NUM_VALUE_BITS)
#define S2N_ERR_T_USAGE_START    (S2N_ERR_T_USAGE << S2N_ERR_NUM_VALUE_BITS)

/* Order of values in this enum is important. New error values should be placed at the end of their respective category.
 * For example, a new TLS protocol related error belongs in the S2N_ERR_T_PROTO category. It should be placed
 * immediately before S2N_ERR_T_INTERNAL_START(the first value of he next category).
 */
typedef enum {
    /* S2N_ERR_T_OK */
    S2N_ERR_OK = S2N_ERR_T_OK_START,
    S2N_ERR_T_OK_END,

    /* S2N_ERR_T_IO */
    S2N_ERR_IO = S2N_ERR_T_IO_START,
    S2N_ERR_T_IO_END,

    /* S2N_ERR_T_CLOSED */
    S2N_ERR_CLOSED = S2N_ERR_T_CLOSED_START,
    S2N_ERR_T_CLOSED_END,

    /* S2N_ERR_T_BLOCKED */
    S2N_ERR_IO_BLOCKED = S2N_ERR_T_BLOCKED_START,
    S2N_ERR_ASYNC_BLOCKED,
    S2N_ERR_EARLY_DATA_BLOCKED,
    S2N_ERR_APP_DATA_BLOCKED,
    S2N_ERR_T_BLOCKED_END,

    /* S2N_ERR_T_ALERT */
    S2N_ERR_ALERT = S2N_ERR_T_ALERT_START,
    S2N_ERR_T_ALERT_END,

    /* S2N_ERR_T_PROTO */
    S2N_ERR_ENCRYPT = S2N_ERR_T_PROTO_START,
    S2N_ERR_DECRYPT,
    S2N_ERR_BAD_MESSAGE,
    S2N_ERR_UNEXPECTED_CERT_REQUEST,
    S2N_ERR_KEY_INIT,
    S2N_ERR_KEY_DESTROY,
    S2N_ERR_DH_SERIALIZING,
    S2N_ERR_DH_SHARED_SECRET,
    S2N_ERR_DH_WRITING_PUBLIC_KEY,
    S2N_ERR_DH_FAILED_SIGNING,
    S2N_ERR_DH_COPYING_PARAMETERS,
    S2N_ERR_DH_GENERATING_PARAMETERS,
    S2N_ERR_CIPHER_NOT_SUPPORTED,
    S2N_ERR_NO_APPLICATION_PROTOCOL,
    S2N_ERR_FALLBACK_DETECTED,
    S2N_ERR_HASH_DIGEST_FAILED,
    S2N_ERR_HASH_INIT_FAILED,
    S2N_ERR_HASH_UPDATE_FAILED,
    S2N_ERR_HASH_COPY_FAILED,
    S2N_ERR_HASH_WIPE_FAILED,
    S2N_ERR_HASH_NOT_READY,
    S2N_ERR_ALLOW_MD5_FOR_FIPS_FAILED,
    S2N_ERR_DECODE_CERTIFICATE,
    S2N_ERR_DECODE_PRIVATE_KEY,
    S2N_ERR_INVALID_HELLO_RETRY,
    S2N_ERR_INVALID_SIGNATURE_ALGORITHM,
    S2N_ERR_INVALID_SIGNATURE_SCHEME,
    S2N_ERR_NO_VALID_SIGNATURE_SCHEME,
    S2N_ERR_CBC_VERIFY,
    S2N_ERR_DH_COPYING_PUBLIC_KEY,
    S2N_ERR_SIGN,
    S2N_ERR_VERIFY_SIGNATURE,
    S2N_ERR_ECDHE_GEN_KEY,
    S2N_ERR_ECDHE_SHARED_SECRET,
    S2N_ERR_ECDHE_UNSUPPORTED_CURVE,
    S2N_ERR_ECDSA_UNSUPPORTED_CURVE,
    S2N_ERR_ECDHE_SERIALIZING,
    S2N_ERR_KEM_UNSUPPORTED_PARAMS,
    S2N_ERR_SHUTDOWN_RECORD_TYPE,
    S2N_ERR_SHUTDOWN_CLOSED,
    S2N_ERR_NON_EMPTY_RENEGOTIATION_INFO,
    S2N_ERR_RECORD_LIMIT,
    S2N_ERR_CERT_UNTRUSTED,
    S2N_ERR_CERT_REVOKED,
    S2N_ERR_CERT_NOT_YET_VALID,
    S2N_ERR_CERT_EXPIRED,
    S2N_ERR_CERT_TYPE_UNSUPPORTED,
    S2N_ERR_CERT_INVALID,
    S2N_ERR_CERT_MAX_CHAIN_DEPTH_EXCEEDED,
    S2N_ERR_CERT_REJECTED,
    S2N_ERR_CRL_LOOKUP_FAILED,
    S2N_ERR_CRL_SIGNATURE,
    S2N_ERR_CRL_ISSUER,
    S2N_ERR_CRL_UNHANDLED_CRITICAL_EXTENSION,
    S2N_ERR_CRL_INVALID_THIS_UPDATE,
    S2N_ERR_CRL_INVALID_NEXT_UPDATE,
    S2N_ERR_CRL_NOT_YET_VALID,
    S2N_ERR_CRL_EXPIRED,
    S2N_ERR_INVALID_MAX_FRAG_LEN,
    S2N_ERR_MAX_FRAG_LEN_MISMATCH,
    S2N_ERR_PROTOCOL_VERSION_UNSUPPORTED,
    S2N_ERR_BAD_KEY_SHARE,
    S2N_ERR_CANCELLED,
    S2N_ERR_PROTOCOL_DOWNGRADE_DETECTED,
    S2N_ERR_MAX_INNER_PLAINTEXT_SIZE,
    S2N_ERR_RECORD_STUFFER_SIZE,
    S2N_ERR_FRAGMENT_LENGTH_TOO_LARGE,
    S2N_ERR_FRAGMENT_LENGTH_TOO_SMALL,
    S2N_ERR_RECORD_STUFFER_NEEDS_DRAINING,
    S2N_ERR_MISSING_EXTENSION,
    S2N_ERR_UNSUPPORTED_EXTENSION,
    S2N_ERR_DUPLICATE_EXTENSION,
    S2N_ERR_MAX_EARLY_DATA_SIZE,
    S2N_ERR_EARLY_DATA_TRIAL_DECRYPT,
    S2N_ERR_NO_RENEGOTIATION,
    S2N_ERR_KTLS_KEYUPDATE,
    S2N_ERR_T_PROTO_END,

    /* S2N_ERR_T_INTERNAL */
    S2N_ERR_MADVISE = S2N_ERR_T_INTERNAL_START,
    S2N_ERR_ALLOC,
    S2N_ERR_MLOCK,
    S2N_ERR_MUNLOCK,
    S2N_ERR_FSTAT,
    S2N_ERR_OPEN,
    S2N_ERR_MMAP,
    S2N_ERR_ATEXIT,
    S2N_ERR_NOMEM,
    S2N_ERR_NULL,
    S2N_ERR_SAFETY,
    S2N_ERR_INITIALIZED,
    S2N_ERR_NOT_INITIALIZED,
    S2N_ERR_RANDOM_UNINITIALIZED,
    S2N_ERR_OPEN_RANDOM,
    S2N_ERR_RESIZE_STATIC_STUFFER,
    S2N_ERR_RESIZE_TAINTED_STUFFER,
    S2N_ERR_STUFFER_OUT_OF_DATA,
    S2N_ERR_STUFFER_IS_FULL,
    S2N_ERR_STUFFER_NOT_FOUND,
    S2N_ERR_STUFFER_HAS_UNPROCESSED_DATA,
    S2N_ERR_HASH_INVALID_ALGORITHM,
    S2N_ERR_PRF_INVALID_ALGORITHM,
    S2N_ERR_PRF_INVALID_SEED,
    S2N_ERR_PRF_DERIVE,
    S2N_ERR_P_HASH_INVALID_ALGORITHM,
    S2N_ERR_P_HASH_INIT_FAILED,
    S2N_ERR_P_HASH_UPDATE_FAILED,
    S2N_ERR_P_HASH_FINAL_FAILED,
    S2N_ERR_P_HASH_WIPE_FAILED,
    S2N_ERR_HMAC_INVALID_ALGORITHM,
    S2N_ERR_HKDF_OUTPUT_SIZE,
    S2N_ERR_HKDF,
    S2N_ERR_ALERT_PRESENT,
    S2N_ERR_HANDSHAKE_STATE,
    S2N_ERR_SHUTDOWN_PAUSED,
    S2N_ERR_SIZE_MISMATCH,
    S2N_ERR_DRBG,
    S2N_ERR_DRBG_REQUEST_SIZE,
    S2N_ERR_KEY_CHECK,
    S2N_ERR_CIPHER_TYPE,
    S2N_ERR_MAP_DUPLICATE,
    S2N_ERR_MAP_IMMUTABLE,
    S2N_ERR_MAP_MUTABLE,
    S2N_ERR_MAP_INVALID_MAP_SIZE,
    S2N_ERR_INITIAL_HMAC,
    S2N_ERR_INVALID_NONCE_TYPE,
    S2N_ERR_UNIMPLEMENTED,
    S2N_ERR_HANDSHAKE_UNREACHABLE,
    S2N_ERR_READ,
    S2N_ERR_WRITE,
    S2N_ERR_BAD_FD,
    S2N_ERR_RDRAND_FAILED,
    S2N_ERR_FAILED_CACHE_RETRIEVAL,
    S2N_ERR_X509_TRUST_STORE,
    S2N_ERR_UNKNOWN_PROTOCOL_VERSION,
    S2N_ERR_NULL_CN_NAME,
    S2N_ERR_NULL_SANS,
    S2N_ERR_CLIENT_HELLO_VERSION,
    S2N_ERR_CLIENT_PROTOCOL_VERSION,
    S2N_ERR_SERVER_PROTOCOL_VERSION,
    S2N_ERR_ACTUAL_PROTOCOL_VERSION,
    S2N_ERR_POLLING_FROM_SOCKET,
    S2N_ERR_RECV_STUFFER_FROM_CONN,
    S2N_ERR_SEND_STUFFER_TO_CONN,
    S2N_ERR_PRECONDITION_VIOLATION,
    S2N_ERR_POSTCONDITION_VIOLATION,
    S2N_ERR_INTEGER_OVERFLOW,
    S2N_ERR_ARRAY_INDEX_OOB,
    S2N_ERR_FREE_STATIC_BLOB,
    S2N_ERR_RESIZE_STATIC_BLOB,
    S2N_ERR_NO_SUPPORTED_LIBCRYPTO_API,
    S2N_ERR_RECORD_LENGTH_TOO_LARGE,
    S2N_ERR_SET_DUPLICATE_VALUE,
    S2N_ERR_INVALID_PARSED_EXTENSIONS,
    S2N_ERR_ASYNC_CALLBACK_FAILED,
    S2N_ERR_ASYNC_MORE_THAN_ONE,
    S2N_ERR_PQ_CRYPTO,
    S2N_ERR_INVALID_CERT_STATE,
    S2N_ERR_INVALID_EARLY_DATA_STATE,
    S2N_ERR_PKEY_CTX_INIT,
    S2N_ERR_FORK_DETECTION_INIT,
    S2N_ERR_RETRIEVE_FORK_GENERATION_NUMBER,
    S2N_ERR_LIBCRYPTO_VERSION_NUMBER_MISMATCH,
    S2N_ERR_LIBCRYPTO_VERSION_NAME_MISMATCH,
    S2N_ERR_OSSL_PROVIDER,
    S2N_ERR_TEST_ASSERTION,
    S2N_ERR_T_INTERNAL_END,

    /* S2N_ERR_T_USAGE */
    S2N_ERR_NO_ALERT = S2N_ERR_T_USAGE_START,
    S2N_ERR_SERVER_MODE,
    S2N_ERR_CLIENT_MODE,
    S2N_ERR_CLIENT_MODE_DISABLED,
    S2N_ERR_TOO_MANY_CERTIFICATES,
    S2N_ERR_TOO_MANY_SIGNATURE_SCHEMES,
    S2N_ERR_CLIENT_AUTH_NOT_SUPPORTED_IN_FIPS_MODE,
    S2N_ERR_INVALID_BASE64,
    S2N_ERR_INVALID_HEX,
    S2N_ERR_INVALID_PEM,
    S2N_ERR_DH_PARAMS_CREATE,
    S2N_ERR_DH_TOO_SMALL,
    S2N_ERR_DH_PARAMETER_CHECK,
    S2N_ERR_INVALID_PKCS3,
    S2N_ERR_NO_CERTIFICATE_IN_PEM,
    S2N_ERR_SERVER_NAME_TOO_LONG,
    S2N_ERR_NUM_DEFAULT_CERTIFICATES,
    S2N_ERR_MULTIPLE_DEFAULT_CERTIFICATES_PER_AUTH_TYPE,
    S2N_ERR_INVALID_CIPHER_PREFERENCES,
    S2N_ERR_INVALID_APPLICATION_PROTOCOL,
    S2N_ERR_KEY_MISMATCH,
    S2N_ERR_SEND_SIZE,
    S2N_ERR_CORK_SET_ON_UNMANAGED,
    S2N_ERR_UNRECOGNIZED_EXTENSION,
    S2N_ERR_EXTENSION_NOT_RECEIVED,
    S2N_ERR_INVALID_SCT_LIST,
    S2N_ERR_INVALID_OCSP_RESPONSE,
    S2N_ERR_UPDATING_EXTENSION,
    S2N_ERR_INVALID_SERIALIZED_SESSION_STATE,
    S2N_ERR_SERIALIZED_SESSION_STATE_TOO_LONG,
    S2N_ERR_SESSION_ID_TOO_LONG,
    S2N_ERR_CLIENT_AUTH_NOT_SUPPORTED_IN_SESSION_RESUMPTION_MODE,
    S2N_ERR_INVALID_TICKET_KEY_LENGTH,
    S2N_ERR_INVALID_TICKET_KEY_NAME_OR_NAME_LENGTH,
    S2N_ERR_TICKET_KEY_NOT_UNIQUE,
    S2N_ERR_TICKET_KEY_LIMIT,
    S2N_ERR_NO_TICKET_ENCRYPT_DECRYPT_KEY,
    S2N_ERR_ENCRYPT_DECRYPT_KEY_SELECTION_FAILED,
    S2N_ERR_KEY_USED_IN_SESSION_TICKET_NOT_FOUND,
    S2N_ERR_SENDING_NST,
    S2N_ERR_INVALID_DYNAMIC_THRESHOLD,
    S2N_ERR_INVALID_ARGUMENT,
    S2N_ERR_NOT_IN_UNIT_TEST,
    S2N_ERR_NOT_IN_TEST,
    S2N_ERR_UNSUPPORTED_CPU,
    S2N_ERR_SESSION_ID_TOO_SHORT,
    S2N_ERR_CONNECTION_CACHING_DISALLOWED,
    S2N_ERR_SESSION_TICKET_NOT_SUPPORTED,
    S2N_ERR_OCSP_NOT_SUPPORTED,
    S2N_ERR_INVALID_SIGNATURE_ALGORITHMS_PREFERENCES,
    S2N_ERR_RSA_PSS_NOT_SUPPORTED,
    S2N_ERR_INVALID_ECC_PREFERENCES,
    S2N_ERR_INVALID_SECURITY_POLICY,
    S2N_ERR_INVALID_KEM_PREFERENCES,
    S2N_ERR_ASYNC_ALREADY_PERFORMED,
    S2N_ERR_ASYNC_NOT_PERFORMED,
    S2N_ERR_ASYNC_WRONG_CONNECTION,
    S2N_ERR_ASYNC_ALREADY_APPLIED,
    S2N_ERR_UNSUPPORTED_WITH_QUIC,
    S2N_ERR_DUPLICATE_PSK_IDENTITIES,
    S2N_ERR_OFFERED_PSKS_TOO_LONG,
    S2N_ERR_INVALID_SESSION_TICKET,
    S2N_ERR_REENTRANCY,
    S2N_ERR_INVALID_STATE,
    S2N_ERR_EARLY_DATA_NOT_ALLOWED,
    S2N_ERR_NO_CERT_FOUND,
    S2N_ERR_CERT_NOT_VALIDATED,
    S2N_ERR_NO_PRIVATE_KEY,
    S2N_ERR_PSK_MODE,
    S2N_ERR_X509_EXTENSION_VALUE_NOT_FOUND,
    S2N_ERR_INVALID_X509_EXTENSION_TYPE,
    S2N_ERR_INSUFFICIENT_MEM_SIZE,
    S2N_ERR_KEYING_MATERIAL_EXPIRED,
    S2N_ERR_SECRET_SCHEDULE_STATE,
    S2N_ERR_CERT_OWNERSHIP,
    S2N_ERR_INTERNAL_LIBCRYPTO_ERROR,
    S2N_ERR_HANDSHAKE_NOT_COMPLETE,
    S2N_ERR_KTLS_MANAGED_IO,
    S2N_ERR_KTLS_UNSUPPORTED_PLATFORM,
    S2N_ERR_KTLS_UNSUPPORTED_CONN,
    S2N_ERR_KTLS_ENABLE,
    S2N_ERR_KTLS_BAD_CMSG,
    S2N_ERR_KTLS_RENEG,
    S2N_ERR_ATOMIC,
    S2N_ERR_KTLS_KEY_LIMIT,
    S2N_ERR_SECURITY_POLICY_INCOMPATIBLE_CERT,
    S2N_ERR_T_USAGE_END,
} s2n_error;

#define S2N_DEBUG_STR_LEN 128

struct s2n_debug_info {
    const char *debug_str;
    const char *source;
};

extern __thread struct s2n_debug_info _s2n_debug_info;

#define TO_STRING(s)   #s
#define STRING_(s)     TO_STRING(s)
#define STRING__LINE__ STRING_(__LINE__)

/* gets the basename of a file path */
/* _S2N_EXTRACT_BASENAME("Error encountered in /path/to/my/file.c") -> "file.c" */
#if !(defined(CBMC) || defined(__TIMING_CONTRACTS__))
    #define _S2N_RSPLIT(subject, c)     (strrchr((subject), c) ? strrchr((subject), c) + 1 : (subject))
    #define _S2N_EXTRACT_BASENAME(path) _S2N_RSPLIT((path) + strlen(_S2N_DEBUG_LINE_PREFIX), '/')
#else
    #define _S2N_EXTRACT_BASENAME(path) path
#endif

#define _S2N_DEBUG_LINE_PREFIX "Error encountered in "
#define _S2N_DEBUG_LINE        _S2N_DEBUG_LINE_PREFIX __FILE__ ":" STRING__LINE__

#define _S2N_ERROR(x)                                                              \
    do {                                                                           \
        _s2n_debug_info.debug_str = _S2N_DEBUG_LINE;                               \
        _s2n_debug_info.source = _S2N_EXTRACT_BASENAME(_s2n_debug_info.debug_str); \
        s2n_errno = (x);                                                           \
        s2n_calculate_stacktrace();                                                \
    } while (0)
#define S2N_ERROR_PRESERVE_ERRNO() \
    do {                           \
        return -1;                 \
    } while (0)
#define S2N_ERROR_IS_BLOCKING(x) (s2n_error_get_type(x) == S2N_ERR_T_BLOCKED)

/* DEPRECATED: use POSIX_BAIL instead */
#define S2N_ERROR(x)     \
    do {                 \
        _S2N_ERROR((x)); \
        return -1;       \
    } while (0)

/* DEPRECATED: use PTR_BAIL instead */
#define S2N_ERROR_PTR(x) \
    do {                 \
        _S2N_ERROR((x)); \
        return NULL;     \
    } while (0)

/* DEPRECATED: use POSIX_ENSURE instead */
#define S2N_ERROR_IF(cond, x) \
    do {                      \
        if (cond) {           \
            S2N_ERROR(x);     \
        }                     \
    } while (0)

/** Calculate and print stacktraces */
struct s2n_stacktrace {
    char **trace;
    int trace_size;
};

bool s2n_stack_traces_enabled();
int s2n_stack_traces_enabled_set(bool newval);

int s2n_calculate_stacktrace(void);
int s2n_print_stacktrace(FILE *fptr);
int s2n_free_stacktrace(void);
int s2n_get_stacktrace(struct s2n_stacktrace *trace);
void s2n_debug_info_reset(void);
