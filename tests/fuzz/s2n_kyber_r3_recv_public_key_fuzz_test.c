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

/* Target Functions: s2n_kem_recv_public_key s2n_kem_encapsulate kyber_512_r3_crypto_kem_enc */

#include "tests/s2n_test.h"
#include "tests/testlib/s2n_testlib.h"
#include "tls/s2n_kem.h"
#include "utils/s2n_safety.h"

/* The valid_public_key in the corpus directory was generated by taking the first public
 * key (count = 0) from kyber_r3.kat and prepending KYBER_512_R3_PUBLIC_KEY_BYTES as two
 * hex-encoded bytes. This is how we would expect it to appear on the wire. */
static struct s2n_kem_params kyber512_r3_draft0_params = { .kem = &s2n_kyber_512_r3, .len_prefixed = true };
static struct s2n_kem_params kyber512_r3_draft5_params = { .kem = &s2n_kyber_512_r3, .len_prefixed = false };

int s2n_fuzz_test(const uint8_t *buf, size_t len)
{
    POSIX_GUARD(s2n_kem_recv_public_key_fuzz_test(buf, len, &kyber512_r3_draft0_params));
    POSIX_GUARD(s2n_kem_recv_public_key_fuzz_test(buf, len, &kyber512_r3_draft5_params));
    return S2N_SUCCESS;
}

S2N_FUZZ_TARGET(NULL, s2n_fuzz_test, NULL)
