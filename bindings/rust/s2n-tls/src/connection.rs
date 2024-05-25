// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::missing_safety_doc)] // TODO add safety docs

use crate::{
    callbacks::*,
    cert_chain::CertificateChain,
    config::Config,
    enums::*,
    error::{Error, Fallible, Pollable},
    security,
};

use core::{
    convert::TryInto,
    fmt,
    mem::{self, ManuallyDrop, MaybeUninit},
    pin::Pin,
    ptr::NonNull,
    task::{Poll, Waker},
    time::Duration,
};
use libc::c_void;
use s2n_tls_sys::*;
use std::{any::Any, ffi::CStr};

mod builder;
pub use builder::*;

macro_rules! static_const_str {
    ($c_chars:expr) => {
        unsafe { CStr::from_ptr($c_chars) }
            .to_str()
            .map_err(|_| Error::INVALID_INPUT)
    };
}

#[non_exhaustive]
#[derive(Debug, PartialEq)]
/// s2n-tls only tracks up to u8::MAX (255) key updates. If any of the fields show
/// 255 updates, then more than 255 updates may have occurred.
pub struct KeyUpdateCount {
    pub send_key_updates: u8,
    pub recv_key_updates: u8,
}

pub struct Connection {
    connection: NonNull<s2n_connection>,
}

impl fmt::Debug for Connection {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut debug = f.debug_struct("Connection");
        if let Ok(handshake) = self.handshake_type() {
            debug.field("handshake_type", &handshake);
        }
        if let Ok(cipher) = self.cipher_suite() {
            debug.field("cipher_suite", &cipher);
        }
        if let Ok(version) = self.actual_protocol_version() {
            debug.field("actual_protocol_version", &version);
        }
        if let Ok(curve) = self.selected_curve() {
            debug.field("selected_curve", &curve);
        }
        debug.finish_non_exhaustive()
    }
}

/// # Safety
///
/// s2n_connection objects can be sent across threads
unsafe impl Send for Connection {}

/// # Sync
///
/// Although NonNull isn't Sync and allows access to mutable pointers even from
/// immutable references, the Connection interface enforces that all mutating
/// methods correctly require &mut self.
///
/// Developers and reviewers MUST ensure that new methods correctly use
/// either &self or &mut self depending on their behavior. No mechanism enforces this.
///
/// Note: Although non-mutating methods like getters should be thread-safe by definition,
/// technically the only thread safety guarantee provided by the underlying C library
/// is that s2n_send and s2n_recv can be called concurrently.
///
unsafe impl Sync for Connection {}

impl Connection {
    /// # Warning
    ///
    /// The newly created connection uses the default security policy.
    /// Consider changing this depending on your security and compatibility requirements
    /// by calling [`Connection::set_security_policy`].
    /// Alternatively, you can use [`crate::config::Builder`], [`crate::config::Builder::set_security_policy`],
    /// and [`Connection::set_config`] to set the policy on the Config instead of on the Connection.
    /// See the s2n-tls usage guide:
    /// <https://aws.github.io/s2n-tls/usage-guide/ch06-security-policies.html>
    pub fn new(mode: Mode) -> Self {
        crate::init::init();

        let connection = unsafe { s2n_connection_new(mode.into()).into_result() }.unwrap();

        unsafe {
            debug_assert! {
                s2n_connection_get_config(connection.as_ptr(), &mut core::ptr::null_mut())
                    .into_result()
                    .is_err()
            }
        }

        let mut connection = Self { connection };
        connection.init_context(mode);
        connection
    }

    fn init_context(&mut self, mode: Mode) {
        let context = Box::new(Context::new(mode));
        let context = Box::into_raw(context) as *mut c_void;
        // allocate a new context object
        unsafe {
            // There should never be an existing context
            debug_assert!(s2n_connection_get_ctx(self.connection.as_ptr())
                .into_result()
                .is_err());

            s2n_connection_set_ctx(self.connection.as_ptr(), context)
                .into_result()
                .unwrap();
        }
    }

    pub fn new_client() -> Self {
        Self::new(Mode::Client)
    }

    pub fn new_server() -> Self {
        Self::new(Mode::Server)
    }

    pub(crate) fn as_ptr(&mut self) -> *mut s2n_connection {
        self.connection.as_ptr()
    }

    /// # Safety
    ///
    /// Caller must ensure s2n_connection is a valid reference to a [`s2n_connection`] object
    pub(crate) unsafe fn from_raw(connection: NonNull<s2n_connection>) -> Self {
        Self { connection }
    }

    pub(crate) fn mode(&self) -> Mode {
        self.context().mode
    }

    /// can be used to configure s2n to either use built-in blinding (set blinding
    /// to Blinding::BuiltIn) or self-service blinding (set blinding to
    /// Blinding::SelfService).
    pub fn set_blinding(&mut self, blinding: Blinding) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_set_blinding(self.connection.as_ptr(), blinding.into()).into_result()
        }?;
        Ok(self)
    }

    /// Reports the remaining nanoseconds before the connection may be gracefully shutdown.
    ///
    /// This method is expected to succeed, but could fail if the
    /// [underlying C call](`s2n_connection_get_delay`) encounters errors.
    /// Failure indicates that calls to [`Self::poll_shutdown`] will also fail and
    /// that a graceful two-way shutdown of the connection will not be possible.
    pub fn remaining_blinding_delay(&self) -> Result<Duration, Error> {
        let nanos = unsafe { s2n_connection_get_delay(self.connection.as_ptr()).into_result() }?;
        Ok(Duration::from_nanos(nanos))
    }

    /// Sets whether or not a Client Certificate should be required to complete the TLS Connection.
    ///
    /// If this is set to ClientAuthType::Optional the server will request a client certificate
    /// but allow the client to not provide one. Rejecting a client certificate when using
    /// ClientAuthType::Optional will terminate the handshake.
    pub fn set_client_auth_type(
        &mut self,
        client_auth_type: ClientAuthType,
    ) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_set_client_auth_type(self.connection.as_ptr(), client_auth_type.into())
                .into_result()
        }?;
        Ok(self)
    }

    /// Attempts to drop the config on the connection.
    ///
    /// # Safety
    ///
    /// The caller must ensure the config associated with the connection was created
    /// with a [`config::Builder`].
    unsafe fn drop_config(&mut self) -> Result<(), Error> {
        let mut prev_config = core::ptr::null_mut();

        // A valid non-null pointer is returned only if the application previously called
        // [`Self::set_config()`].
        if s2n_connection_get_config(self.connection.as_ptr(), &mut prev_config)
            .into_result()
            .is_ok()
        {
            let prev_config = NonNull::new(prev_config).expect(
                "config should exist since the call to s2n_connection_get_config was successful",
            );
            drop(Config::from_raw(prev_config));
        }

        Ok(())
    }

    /// Associates a configuration object with a connection.
    pub fn set_config(&mut self, mut config: Config) -> Result<&mut Self, Error> {
        unsafe {
            // attempt to drop the currently set config
            self.drop_config()?;

            s2n_connection_set_config(self.connection.as_ptr(), config.as_mut_ptr())
                .into_result()?;

            debug_assert! {
                s2n_connection_get_config(self.connection.as_ptr(), &mut core::ptr::null_mut()).into_result().is_ok(),
                "s2n_connection_set_config was successful"
            };

            // Setting the config on the connection creates one additional reference to the config
            // so do not drop so prevent Rust from calling `drop()` at the end of this function.
            mem::forget(config);
        }

        Ok(self)
    }

    pub(crate) fn config(&self) -> Option<Config> {
        let mut raw = core::ptr::null_mut();
        let config = unsafe {
            s2n_connection_get_config(self.connection.as_ptr(), &mut raw)
                .into_result()
                .ok()?;
            let raw = NonNull::new(raw)?;
            Config::from_raw(raw)
        };
        // Because the config pointer is still set on the connection, this is a copy,
        // not the original config. This is fine -- Configs are immutable.
        let _ = ManuallyDrop::new(config.clone());
        Some(config)
    }

    pub fn set_security_policy(&mut self, policy: &security::Policy) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_set_cipher_preferences(
                self.connection.as_ptr(),
                policy.as_cstr().as_ptr(),
            )
            .into_result()
        }?;
        Ok(self)
    }

    /// provides a smooth transition from s2n_connection_prefer_low_latency to s2n_connection_prefer_throughput.
    ///
    /// s2n_send uses small TLS records that fit into a single TCP segment for the resize_threshold
    /// bytes (cap to 8M) of data and reset record size back to a single segment after timeout_threshold
    /// seconds of inactivity.
    pub fn set_dynamic_record_threshold(
        &mut self,
        resize_threshold: u32,
        timeout_threshold: u16,
    ) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_set_dynamic_record_threshold(
                self.connection.as_ptr(),
                resize_threshold,
                timeout_threshold,
            )
            .into_result()
        }?;
        Ok(self)
    }

    /// Signals the connection to do a key_update at the next possible opportunity.
    /// Note that the resulting key update message will not be sent until `send` is
    /// called on the connection.
    ///
    /// `peer_request` indicates if a key update should also be requested
    /// of the peer. When set to `KeyUpdateNotRequested`, then only the sending
    /// key of the connection will be updated. If set to `KeyUpdateRequested`, then
    /// the sending key of conn will be updated AND the peer will be requested to
    /// update their sending key. Note that s2n-tls currently only supports
    /// `peer_request` being set to `KeyUpdateNotRequested` and will return an error
    /// if any other value is used.
    pub fn request_key_update(&mut self, peer_request: PeerKeyUpdate) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_request_key_update(self.connection.as_ptr(), peer_request.into())
                .into_result()
        }?;
        Ok(self)
    }

    /// Reports the number of times sending and receiving keys have been updated.
    ///
    /// This only applies to TLS1.3. Earlier versions do not support key updates.
    #[cfg(feature = "unstable-ktls")]
    pub fn key_update_counts(&self) -> Result<KeyUpdateCount, Error> {
        let mut send_key_updates = 0;
        let mut recv_key_updates = 0;
        unsafe {
            s2n_connection_get_key_update_counts(
                self.connection.as_ptr(),
                &mut send_key_updates,
                &mut recv_key_updates,
            )
            .into_result()?;
        }
        Ok(KeyUpdateCount {
            send_key_updates,
            recv_key_updates,
        })
    }

    /// sets the application protocol preferences on an s2n_connection object.
    ///
    /// protocols is a list in order of preference, with most preferred protocol first, and of
    /// length protocol_count. When acting as a client the protocol list is included in the
    /// Client Hello message as the ALPN extension. As a server, the list is used to negotiate
    /// a mutual application protocol with the client. After the negotiation for the connection has
    /// completed, the agreed upon protocol can be retrieved with s2n_get_application_protocol
    pub fn set_application_protocol_preference<P: IntoIterator<Item = I>, I: AsRef<[u8]>>(
        &mut self,
        protocols: P,
    ) -> Result<&mut Self, Error> {
        // reset the list
        unsafe {
            s2n_connection_set_protocol_preferences(self.connection.as_ptr(), core::ptr::null(), 0)
                .into_result()
        }?;

        for protocol in protocols {
            self.append_application_protocol_preference(protocol.as_ref())?;
        }

        Ok(self)
    }

    pub fn append_application_protocol_preference(
        &mut self,
        protocol: &[u8],
    ) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_append_protocol_preference(
                self.connection.as_ptr(),
                protocol.as_ptr(),
                protocol
                    .len()
                    .try_into()
                    .map_err(|_| Error::INVALID_INPUT)?,
            )
            .into_result()
        }?;
        Ok(self)
    }

    /// may be used to receive data with callbacks defined by the user.
    pub fn set_receive_callback(&mut self, callback: s2n_recv_fn) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_set_recv_cb(self.connection.as_ptr(), callback).into_result() }?;
        Ok(self)
    }

    /// # Safety
    ///
    /// The `context` pointer must live at least as long as the connection
    pub unsafe fn set_receive_context(&mut self, context: *mut c_void) -> Result<&mut Self, Error> {
        s2n_connection_set_recv_ctx(self.connection.as_ptr(), context).into_result()?;
        Ok(self)
    }

    /// may be used to receive data with callbacks defined by the user.
    pub fn set_send_callback(&mut self, callback: s2n_send_fn) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_set_send_cb(self.connection.as_ptr(), callback).into_result() }?;
        Ok(self)
    }

    /// Sets the callback to use for verifying that a hostname from an X.509 certificate is
    /// trusted.
    ///
    /// The callback may be called more than once during certificate validation as each SAN on
    /// the certificate will be checked.
    ///
    /// Corresponds to the underlying C API
    /// [s2n_connection_set_verify_host_callback](https://aws.github.io/s2n-tls/doxygen/s2n_8h.html).
    pub fn set_verify_host_callback<T: 'static + VerifyHostNameCallback>(
        &mut self,
        handler: T,
    ) -> Result<&mut Self, Error> {
        unsafe extern "C" fn verify_host_cb_fn(
            host_name: *const ::libc::c_char,
            host_name_len: usize,
            context: *mut ::libc::c_void,
        ) -> u8 {
            let context = &mut *(context as *mut Context);
            let handler = context.verify_host_callback.as_mut().unwrap();
            verify_host(host_name, host_name_len, handler)
        }

        self.context_mut().verify_host_callback = Some(Box::new(handler));
        unsafe {
            s2n_connection_set_verify_host_callback(
                self.connection.as_ptr(),
                Some(verify_host_cb_fn),
                self.context_mut() as *mut Context as *mut c_void,
            )
            .into_result()
        }?;
        Ok(self)
    }

    /// # Safety
    ///
    /// The `context` pointer must live at least as long as the connection
    pub unsafe fn set_send_context(&mut self, context: *mut c_void) -> Result<&mut Self, Error> {
        s2n_connection_set_send_ctx(self.connection.as_ptr(), context).into_result()?;
        Ok(self)
    }

    /// Connections prefering low latency will be encrypted using small record sizes that
    /// can be decrypted sooner by the recipient.
    pub fn prefer_low_latency(&mut self) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_prefer_low_latency(self.connection.as_ptr()).into_result() }?;
        Ok(self)
    }

    /// Connections prefering throughput will use large record sizes that minimize overhead.
    pub fn prefer_throughput(&mut self) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_prefer_throughput(self.connection.as_ptr()).into_result() }?;
        Ok(self)
    }

    /// wipes and free the in and out buffers associated with a connection.
    ///
    /// This function may be called when a connection is in keep-alive or idle state to
    /// reduce memory overhead of long lived connections.
    pub fn release_buffers(&mut self) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_release_buffers(self.connection.as_ptr()).into_result() }?;
        Ok(self)
    }

    pub fn use_corked_io(&mut self) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_use_corked_io(self.connection.as_ptr()).into_result() }?;
        Ok(self)
    }

    /// wipes an existing connection and allows it to be reused.
    ///
    /// This method erases all data associated with a connection including pending reads.
    /// This function should be called after all I/O is completed and s2n_shutdown has been
    /// called. Reusing the same connection handle(s) is more performant than repeatedly
    /// calling s2n_connection_new and s2n_connection_free
    pub fn wipe(&mut self) -> Result<&mut Self, Error> {
        let mode = self.mode();
        unsafe {
            // Wiping the connection will wipe the pointer to the context,
            // so retrieve and drop that memory first.
            let ctx = self.context_mut();
            drop(Box::from_raw(ctx));

            s2n_connection_wipe(self.connection.as_ptr()).into_result()
        }?;

        self.init_context(mode);
        Ok(self)
    }

    /// Performs the TLS handshake to completion
    ///
    /// Multiple callbacks can be configured for a connection and config, but
    /// [`Self::poll_negotiate()`] can only execute and block on one callback at a time.
    /// The handshake is sequential, not concurrent, and stops execution when
    /// it encounters an async callback.
    ///
    /// The handshake does not continue execution (and therefore can't call
    /// any other callbacks) until the blocking async task reports completion.
    pub fn poll_negotiate(&mut self) -> Poll<Result<&mut Self, Error>> {
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        if !core::mem::replace(&mut self.context_mut().connection_initialized, true) {
            if let Some(config) = self.config() {
                if let Some(callback) = config.context().connection_initializer.as_ref() {
                    let future = callback.initialize_connection(self);
                    AsyncCallback::trigger(future, self);
                }
            }
        }

        loop {
            // check if an async task exists and poll it to completion
            if let Some(fut) = self.poll_async_task() {
                match fut {
                    Poll::Ready(Ok(())) => {
                        // happy case:
                        // continue and call s2n_negotiate to make progress on the handshake
                    }
                    Poll::Ready(Err(err)) => {
                        // error case:
                        // if the callback returned an error then abort the handshake
                        return Poll::Ready(Err(err));
                    }
                    Poll::Pending => return Poll::Pending,
                }
            }

            let res = unsafe { s2n_negotiate(self.connection.as_ptr(), &mut blocked).into_poll() };

            match res {
                Poll::Ready(res) => {
                    let res = res.map(|_| self);
                    return Poll::Ready(res);
                }
                Poll::Pending => {
                    // if there is no connection_future then return, otherwise continue
                    // looping and polling the future
                    if self.context_mut().async_callback.is_none() {
                        return Poll::Pending;
                    }
                }
            }
        }
    }

    // Poll the connection future if it exists.
    //
    // If the future returns Pending, then re-set it back on the Connection.
    fn poll_async_task(&mut self) -> Option<Poll<Result<(), Error>>> {
        self.take_async_callback().map(|mut callback| {
            let waker = self.waker().ok_or(Error::MISSING_WAKER)?.clone();
            let mut ctx = core::task::Context::from_waker(&waker);
            match Pin::new(&mut callback).poll(self, &mut ctx) {
                Poll::Ready(result) => Poll::Ready(result),
                Poll::Pending => {
                    // replace the future if it hasn't completed yet
                    self.set_async_callback(callback);
                    Poll::Pending
                }
            }
        })
    }

    /// Encrypts and sends data on a connection where
    /// [negotiate](`Self::poll_negotiate`) has succeeded.
    ///
    /// Returns the number of bytes written, and may indicate a partial write.
    pub fn poll_send(&mut self, buf: &[u8]) -> Poll<Result<usize, Error>> {
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        let buf_len: isize = buf.len().try_into().map_err(|_| Error::INVALID_INPUT)?;
        let buf_ptr = buf.as_ptr() as *const ::libc::c_void;
        unsafe { s2n_send(self.connection.as_ptr(), buf_ptr, buf_len, &mut blocked).into_poll() }
    }

    /// Reads and decrypts data from a connection where
    /// [negotiate](`Self::poll_negotiate`) has succeeded.
    ///
    /// Returns the number of bytes read, and may indicate a partial read.
    /// 0 bytes returned indicates EOF due to connection closure.
    pub fn poll_recv(&mut self, buf: &mut [u8]) -> Poll<Result<usize, Error>> {
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        let buf_len: isize = buf.len().try_into().map_err(|_| Error::INVALID_INPUT)?;
        let buf_ptr = buf.as_ptr() as *mut ::libc::c_void;
        unsafe { s2n_recv(self.connection.as_ptr(), buf_ptr, buf_len, &mut blocked).into_poll() }
    }

    /// Reads and decrypts data from a connection where
    /// [negotiate](`Self::poll_negotiate`) has succeeded
    /// to a uninitialized buffer.
    ///
    /// Returns the number of bytes read, and may indicate a partial read.
    /// 0 bytes returned indicates EOF due to connection closure.
    ///
    /// Safety: this function is always safe to call, and additionally:
    /// 1. It will never deinitialize any bytes in `buf`.
    /// 2. If it returns `Ok(n)`, then the first `n` bytes of `buf`
    /// will have been initialized by this function.
    pub fn poll_recv_uninitialized(
        &mut self,
        buf: &mut [MaybeUninit<u8>],
    ) -> Poll<Result<usize, Error>> {
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        let buf_len: isize = buf.len().try_into().map_err(|_| Error::INVALID_INPUT)?;
        let buf_ptr = buf.as_ptr() as *mut ::libc::c_void;

        // Safety:
        // 1. s2n_recv never writes uninitialized garbage to `buf`.
        // 2. if s2n_recv returns `+n`, it guarantees that the first
        // `n` bytes of `buf` have been initialized, which allows this
        // function to return `Ok(n)`
        unsafe { s2n_recv(self.connection.as_ptr(), buf_ptr, buf_len, &mut blocked).into_poll() }
    }

    /// Attempts to flush any data previously buffered by a call to [send](`Self::poll_send`).
    pub fn poll_flush(&mut self) -> Poll<Result<&mut Self, Error>> {
        self.poll_send(&[0; 0]).map_ok(|_| self)
    }

    /// Gets the number of bytes that are currently available in the buffer to be read.
    pub fn peek_len(&self) -> usize {
        unsafe { s2n_peek(self.connection.as_ptr()) as usize }
    }

    /// Attempts a graceful shutdown of the TLS connection.
    ///
    /// The shutdown is not complete until the necessary shutdown messages
    /// have been successfully sent and received. If the peer does not respond
    /// correctly, the graceful shutdown may fail.
    pub fn poll_shutdown(&mut self) -> Poll<Result<&mut Self, Error>> {
        if !self.remaining_blinding_delay()?.is_zero() {
            return Poll::Pending;
        }
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        unsafe {
            s2n_shutdown(self.connection.as_ptr(), &mut blocked)
                .into_poll()
                .map_ok(|_| self)
        }
    }

    /// Attempts a graceful shutdown of the write side of a TLS connection.
    ///
    /// Unlike Self::poll_shutdown, no reponse from the peer is necessary.
    /// If using TLS1.3, the connection can continue to be used for reading afterwards.
    pub fn poll_shutdown_send(&mut self) -> Poll<Result<&mut Self, Error>> {
        if !self.remaining_blinding_delay()?.is_zero() {
            return Poll::Pending;
        }
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        unsafe {
            s2n_shutdown_send(self.connection.as_ptr(), &mut blocked)
                .into_poll()
                .map_ok(|_| self)
        }
    }

    /// Returns the TLS alert code, if any
    pub fn alert(&self) -> Option<u8> {
        let alert =
            unsafe { s2n_connection_get_alert(self.connection.as_ptr()).into_result() }.ok()?;
        Some(alert as u8)
    }

    /// Sets the server name value for the connection
    pub fn set_server_name(&mut self, server_name: &str) -> Result<&mut Self, Error> {
        let server_name = std::ffi::CString::new(server_name).map_err(|_| Error::INVALID_INPUT)?;
        unsafe {
            s2n_set_server_name(self.connection.as_ptr(), server_name.as_ptr()).into_result()
        }?;
        Ok(self)
    }

    /// Get the server name associated with the connection client hello.
    pub fn server_name(&self) -> Option<&str> {
        unsafe {
            let server_name = s2n_get_server_name(self.connection.as_ptr());
            match server_name.into_result() {
                Ok(server_name) => CStr::from_ptr(server_name).to_str().ok(),
                Err(_) => None,
            }
        }
    }

    /// Adds a session ticket from a previous TLS connection to create a resumed session
    pub fn set_session_ticket(&mut self, session: &[u8]) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_set_session(self.connection.as_ptr(), session.as_ptr(), session.len())
                .into_result()
        }?;
        Ok(self)
    }

    /// Retrieves the size of the session ticket.
    pub fn session_ticket_length(&self) -> Result<usize, Error> {
        let len =
            unsafe { s2n_connection_get_session_length(self.connection.as_ptr()).into_result()? };
        Ok(len.try_into().unwrap())
    }

    /// Serializes the session state from the connection into `output` and returns
    /// the length of the session ticket.
    ///
    /// If the buffer does not have the size for the session_ticket,
    /// `Error::INVALID_INPUT` is returned.
    ///
    /// Note: This function is not recommended for > TLS1.2 because in TLS1.3
    /// servers can send multiple session tickets and this will return only
    /// the most recently received ticket.
    pub fn session_ticket(&self, output: &mut [u8]) -> Result<usize, Error> {
        if output.len() < self.session_ticket_length()? {
            return Err(Error::INVALID_INPUT);
        }
        let written = unsafe {
            s2n_connection_get_session(self.connection.as_ptr(), output.as_mut_ptr(), output.len())
                .into_result()?
        };
        Ok(written.try_into().unwrap())
    }

    /// Sets a Waker on the connection context or clears it if `None` is passed.
    pub fn set_waker(&mut self, waker: Option<&Waker>) -> Result<&mut Self, Error> {
        let ctx = self.context_mut();

        if let Some(waker) = waker {
            if let Some(prev_waker) = ctx.waker.as_mut() {
                // only replace the Waker if they dont reference the same task
                if !prev_waker.will_wake(waker) {
                    prev_waker.clone_from(waker);
                }
            } else {
                ctx.waker = Some(waker.clone());
            }
        } else {
            ctx.waker = None;
        }
        Ok(self)
    }

    /// Returns the Waker set on the connection context.
    pub fn waker(&self) -> Option<&Waker> {
        let ctx = self.context();
        ctx.waker.as_ref()
    }

    /// Takes the [`Option::take`] the connection_future stored on the
    /// connection context.
    ///
    /// If the Future returns `Poll::Pending` and has not completed, then it
    /// should be re-set using [`Self::set_connection_future()`]
    fn take_async_callback(&mut self) -> Option<AsyncCallback> {
        let ctx = self.context_mut();
        ctx.async_callback.take()
    }

    /// Sets a `connection_future` on the connection context.
    pub(crate) fn set_async_callback(&mut self, callback: AsyncCallback) {
        let ctx = self.context_mut();
        debug_assert!(ctx.async_callback.is_none());
        ctx.async_callback = Some(callback);
    }

    /// Retrieve a mutable reference to the [`Context`] stored on the connection.
    fn context_mut(&mut self) -> &mut Context {
        unsafe {
            let ctx = s2n_connection_get_ctx(self.connection.as_ptr())
                .into_result()
                .unwrap();
            &mut *(ctx.as_ptr() as *mut Context)
        }
    }

    /// Retrieve a reference to the [`Context`] stored on the connection.
    fn context(&self) -> &Context {
        unsafe {
            let ctx = s2n_connection_get_ctx(self.connection.as_ptr())
                .into_result()
                .unwrap();
            &*(ctx.as_ptr() as *mut Context)
        }
    }

    /// Mark that the server_name extension was used to configure the connection.
    pub fn server_name_extension_used(&mut self) {
        // TODO: requiring the application to call this method is a pretty sharp edge.
        // Figure out if its possible to automatically call this from the Rust bindings.
        unsafe {
            s2n_connection_server_name_extension_used(self.connection.as_ptr())
                .into_result()
                .unwrap();
        }
    }

    /// Check if client auth was used for a connection.
    ///
    /// This is only relevant if [`ClientAuthType::Optional] was used.
    pub fn client_cert_used(&self) -> bool {
        unsafe { s2n_connection_client_cert_used(self.connection.as_ptr()) == 1 }
    }

    /// Retrieves the raw bytes of the client cert chain received from the peer, if present.
    pub fn client_cert_chain_bytes(&self) -> Result<Option<&[u8]>, Error> {
        if !self.client_cert_used() {
            return Ok(None);
        }

        let mut chain = std::ptr::null_mut();
        let mut len = 0;
        unsafe {
            s2n_connection_get_client_cert_chain(self.connection.as_ptr(), &mut chain, &mut len)
                .into_result()?;
        }

        if chain.is_null() || len == 0 {
            return Ok(None);
        }

        unsafe { Ok(Some(std::slice::from_raw_parts(chain, len as usize))) }
    }

    // The memory backing the ClientHello is owned by the Connection, so we
    // tie the ClientHello to the lifetime of the Connection. This is validated
    // with a doc test that ensures the ClientHello is invalid once the
    // connection has gone out of scope.
    //
    /// Returns a reference to the ClientHello associated with the connection.
    /// ```compile_fail
    /// use s2n_tls::client_hello::ClientHello;
    /// use s2n_tls::connection::Connection;
    /// use s2n_tls::enums::Mode;
    ///
    /// let mut conn = Connection::new(Mode::Server);
    /// let mut client_hello: &ClientHello = conn.client_hello().unwrap();
    /// drop(conn);
    /// client_hello.raw_message();
    /// ```
    ///
    /// The compilation could be failing for a variety of reasons, so make sure
    /// that the test case is actually good.
    /// ```no_run
    /// use s2n_tls::client_hello::ClientHello;
    /// use s2n_tls::connection::Connection;
    /// use s2n_tls::enums::Mode;
    ///
    /// let mut conn = Connection::new(Mode::Server);
    /// let mut client_hello: &ClientHello = conn.client_hello().unwrap();
    /// client_hello.raw_message();
    /// drop(conn);
    /// ```
    pub fn client_hello(&self) -> Result<&crate::client_hello::ClientHello, Error> {
        let mut handle =
            unsafe { s2n_connection_get_client_hello(self.connection.as_ptr()).into_result()? };
        Ok(crate::client_hello::ClientHello::from_ptr(unsafe {
            handle.as_mut()
        }))
    }

    pub(crate) fn mark_client_hello_cb_done(&mut self) -> Result<(), Error> {
        unsafe {
            s2n_client_hello_cb_done(self.connection.as_ptr()).into_result()?;
        }
        Ok(())
    }

    pub fn actual_protocol_version(&self) -> Result<Version, Error> {
        let version = unsafe {
            s2n_connection_get_actual_protocol_version(self.connection.as_ptr()).into_result()?
        };
        version.try_into()
    }

    pub fn handshake_type(&self) -> Result<&str, Error> {
        let handshake = unsafe {
            s2n_connection_get_handshake_type_name(self.connection.as_ptr()).into_result()?
        };
        // The strings returned by s2n_connection_get_handshake_type_name
        // are static and immutable after they are first calculated
        static_const_str!(handshake)
    }

    pub fn cipher_suite(&self) -> Result<&str, Error> {
        let cipher = unsafe { s2n_connection_get_cipher(self.connection.as_ptr()).into_result()? };
        // The strings returned by s2n_connection_get_cipher
        // are static and immutable since they are const fields on static const structs
        static_const_str!(cipher)
    }

    pub fn selected_curve(&self) -> Result<&str, Error> {
        let curve = unsafe { s2n_connection_get_curve(self.connection.as_ptr()).into_result()? };
        static_const_str!(curve)
    }

    pub fn selected_signature_algorithm(&self) -> Result<SignatureAlgorithm, Error> {
        let mut sig_alg = s2n_tls_signature_algorithm::ANONYMOUS;
        unsafe {
            s2n_connection_get_selected_signature_algorithm(self.connection.as_ptr(), &mut sig_alg)
                .into_result()?;
        }
        sig_alg.try_into()
    }

    pub fn selected_hash_algorithm(&self) -> Result<HashAlgorithm, Error> {
        let mut hash_alg = s2n_tls_hash_algorithm::NONE;
        unsafe {
            s2n_connection_get_selected_digest_algorithm(self.connection.as_ptr(), &mut hash_alg)
                .into_result()?;
        }
        hash_alg.try_into()
    }

    pub fn selected_client_signature_algorithm(&self) -> Result<Option<SignatureAlgorithm>, Error> {
        let mut sig_alg = s2n_tls_signature_algorithm::ANONYMOUS;
        unsafe {
            s2n_connection_get_selected_client_cert_signature_algorithm(
                self.connection.as_ptr(),
                &mut sig_alg,
            )
            .into_result()?;
        }
        Ok(match sig_alg {
            s2n_tls_signature_algorithm::ANONYMOUS => None,
            sig_alg => Some(sig_alg.try_into()?),
        })
    }

    pub fn selected_client_hash_algorithm(&self) -> Result<Option<HashAlgorithm>, Error> {
        let mut hash_alg = s2n_tls_hash_algorithm::NONE;
        unsafe {
            s2n_connection_get_selected_client_cert_digest_algorithm(
                self.connection.as_ptr(),
                &mut hash_alg,
            )
            .into_result()?;
        }
        Ok(match hash_alg {
            s2n_tls_hash_algorithm::NONE => None,
            hash_alg => Some(hash_alg.try_into()?),
        })
    }

    /// Provides access to the TLS-Exporter functionality.
    ///
    /// See https://datatracker.ietf.org/doc/html/rfc5705 and https://www.rfc-editor.org/rfc/rfc8446.
    ///
    /// This is currently only available with TLS 1.3 connections which have finished a handshake.
    pub fn tls_exporter(
        &self,
        label: &[u8],
        context: &[u8],
        output: &mut [u8],
    ) -> Result<(), Error> {
        unsafe {
            s2n_connection_tls_exporter(
                self.connection.as_ptr(),
                label.as_ptr(),
                label.len().try_into().map_err(|_| Error::INVALID_INPUT)?,
                context.as_ptr(),
                context.len().try_into().map_err(|_| Error::INVALID_INPUT)?,
                output.as_mut_ptr(),
                output.len().try_into().map_err(|_| Error::INVALID_INPUT)?,
            )
            .into_result()
            .map(|_| ())
        }
    }

    /// Returns the validated peer certificate chain.
    // 'static lifetime is because this copies the certificate chain from the connection into a new
    // chain, so the lifetime is independent of the connection.
    pub fn peer_cert_chain(&self) -> Result<CertificateChain<'static>, Error> {
        unsafe {
            let mut chain = CertificateChain::new()?;
            s2n_connection_get_peer_cert_chain(
                self.connection.as_ptr(),
                chain.as_mut_ptr().as_ptr(),
            )
            .into_result()
            .map(|_| ())?;
            Ok(chain)
        }
    }

    /// Get the certificate used during the TLS handshake
    ///
    /// - If `self` is a server connection, the certificate selected will depend on the
    ///   ServerName sent by the client and supported ciphers.
    /// - If `self` is a client connection, the certificate sent in response to a CertificateRequest
    ///   message is returned. Currently s2n-tls supports loading only one certificate in client mode. Note that
    ///   not all TLS endpoints will request a certificate.
    pub fn selected_cert(&self) -> Option<CertificateChain<'_>> {
        unsafe {
            // The API only returns null, no error is actually set.
            // Clippy doesn't realize from_ptr_reference is unsafe.
            #[allow(clippy::manual_map)]
            if let Some(ptr) =
                NonNull::new(s2n_connection_get_selected_cert(self.connection.as_ptr()))
            {
                Some(CertificateChain::from_ptr_reference(ptr))
            } else {
                None
            }
        }
    }

    pub fn master_secret(&self) -> Result<Vec<u8>, Error> {
        // TLS1.2 master secrets are always 48 bytes
        let mut secret = vec![0; 48];
        unsafe {
            s2n_connection_get_master_secret(
                self.connection.as_ptr(),
                secret.as_mut_ptr(),
                secret.len(),
            )
            .into_result()?;
        }
        Ok(secret)
    }

    /// Retrieves the size of the serialized connection
    pub fn serialization_length(&self) -> Result<usize, Error> {
        unsafe {
            let mut length = 0;
            s2n_connection_serialization_length(self.connection.as_ptr(), &mut length)
                .into_result()?;
            Ok(length.try_into().unwrap())
        }
    }

    /// Serializes the TLS connection into the provided buffer
    pub fn serialize(&self, output: &mut [u8]) -> Result<(), Error> {
        unsafe {
            s2n_connection_serialize(
                self.connection.as_ptr(),
                output.as_mut_ptr(),
                output.len().try_into().map_err(|_| Error::INVALID_INPUT)?,
            )
            .into_result()?;
            Ok(())
        }
    }

    /// Deserializes the input buffer into a new TLS connection that can send/recv
    /// data from the original peer.
    pub fn deserialize(&mut self, input: &[u8]) -> Result<(), Error> {
        let size = input.len();
        /* This is not ideal, we know that s2n_connection_deserialize will not mutate the
         * input value, however, the mut is needed to use the stuffer functions. */
        let input = input.as_ptr() as *mut u8;
        unsafe {
            s2n_connection_deserialize(
                self.as_ptr(),
                input,
                size.try_into().map_err(|_| Error::INVALID_INPUT)?,
            )
            .into_result()?;
            Ok(())
        }
    }

    /// Determines whether the connection was resumed from an earlier handshake.
    pub fn resumed(&self) -> bool {
        unsafe { s2n_connection_is_session_resumed(self.connection.as_ptr()) == 1 }
    }

    /// Associates an arbitrary application context with the Connection to be later retrieved via
    /// the [`Self::application_context()`] and [`Self::application_context_mut()`] APIs.
    ///
    /// This API will override an existing application context set on the Connection.
    pub fn set_application_context<T: Send + Sync + 'static>(&mut self, app_context: T) {
        self.context_mut().app_context = Some(Box::new(app_context));
    }

    /// Retrieves a reference to the application context associated with the Connection.
    ///
    /// If an application context hasn't already been set on the Connection, or if the set
    /// application context isn't of type T, None will be returned.
    ///
    /// To set a context on the connection, use [`Self::set_application_context()`]. To retrieve a
    /// mutable reference to the context, use [`Self::application_context_mut()`].
    pub fn application_context<T: Send + Sync + 'static>(&self) -> Option<&T> {
        match self.context().app_context.as_ref() {
            None => None,
            // The Any trait keeps track of the application context's type. downcast_ref() returns
            // Some only if the correct type is provided:
            // https://doc.rust-lang.org/std/any/trait.Any.html#method.downcast_ref
            Some(app_context) => app_context.downcast_ref::<T>(),
        }
    }

    /// Retrieves a mutable reference to the application context associated with the Connection.
    ///
    /// If an application context hasn't already been set on the Connection, or if the set
    /// application context isn't of type T, None will be returned.
    ///
    /// To set a context on the connection, use [`Self::set_application_context()`]. To retrieve an
    /// immutable reference to the context, use [`Self::application_context()`].
    pub fn application_context_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        match self.context_mut().app_context.as_mut() {
            None => None,
            Some(app_context) => app_context.downcast_mut::<T>(),
        }
    }
}

struct Context {
    mode: Mode,
    waker: Option<Waker>,
    async_callback: Option<AsyncCallback>,
    verify_host_callback: Option<Box<dyn VerifyHostNameCallback>>,
    connection_initialized: bool,
    app_context: Option<Box<dyn Any + Send + Sync>>,
}

impl Context {
    fn new(mode: Mode) -> Self {
        Context {
            mode,
            waker: None,
            async_callback: None,
            verify_host_callback: None,
            connection_initialized: false,
            app_context: None,
        }
    }
}

#[cfg(feature = "quic")]
impl Connection {
    pub fn enable_quic(&mut self) -> Result<&mut Self, Error> {
        unsafe { s2n_connection_enable_quic(self.connection.as_ptr()).into_result() }?;
        Ok(self)
    }

    pub fn set_quic_transport_parameters(&mut self, buffer: &[u8]) -> Result<&mut Self, Error> {
        unsafe {
            s2n_connection_set_quic_transport_parameters(
                self.connection.as_ptr(),
                buffer.as_ptr(),
                buffer.len().try_into().map_err(|_| Error::INVALID_INPUT)?,
            )
            .into_result()
        }?;
        Ok(self)
    }

    pub fn quic_transport_parameters(&mut self) -> Result<&[u8], Error> {
        let mut ptr = core::ptr::null();
        let mut len = 0;
        unsafe {
            s2n_connection_get_quic_transport_parameters(
                self.connection.as_ptr(),
                &mut ptr,
                &mut len,
            )
            .into_result()
        }?;
        let buffer = unsafe { core::slice::from_raw_parts(ptr, len as _) };
        Ok(buffer)
    }

    /// # Safety
    ///
    /// The `context` pointer must live at least as long as the connection
    pub unsafe fn set_secret_callback(
        &mut self,
        callback: s2n_secret_cb,
        context: *mut c_void,
    ) -> Result<&mut Self, Error> {
        s2n_connection_set_secret_callback(self.connection.as_ptr(), callback, context)
            .into_result()?;
        Ok(self)
    }

    pub fn quic_process_post_handshake_message(&mut self) -> Result<&mut Self, Error> {
        let mut blocked = s2n_blocked_status::NOT_BLOCKED;
        unsafe {
            s2n_recv_quic_post_handshake_message(self.connection.as_ptr(), &mut blocked)
                .into_result()
        }?;
        Ok(self)
    }

    /// Allows the quic library to check if session tickets are expected
    pub fn are_session_tickets_enabled(&self) -> bool {
        unsafe { s2n_connection_are_session_tickets_enabled(self.connection.as_ptr()) }
    }
}

impl AsRef<Connection> for Connection {
    fn as_ref(&self) -> &Connection {
        self
    }
}

impl AsMut<Connection> for Connection {
    fn as_mut(&mut self) -> &mut Connection {
        self
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // ignore failures since there's not much we can do about it
        unsafe {
            // clean up context
            let prev_ctx = self.context_mut();
            drop(Box::from_raw(prev_ctx));
            let _ = s2n_connection_set_ctx(self.connection.as_ptr(), core::ptr::null_mut())
                .into_result();

            // cleanup config
            let _ = self.drop_config();

            // cleanup connection
            let _ = s2n_connection_free(self.connection.as_ptr()).into_result();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ensure the connection context is send
    #[test]
    fn context_send_test() {
        fn assert_send<T: 'static + Send>() {}
        assert_send::<Context>();
    }

    // ensure the connection context is sync
    #[test]
    fn context_sync_test() {
        fn assert_sync<T: 'static + Sync>() {}
        assert_sync::<Context>();
    }

    /// Test that an application context can be set and retrieved.
    #[test]
    fn test_app_context_set_and_retrieve() {
        let mut connection = Connection::new_server();

        // Before a context is set, None is returned.
        assert!(connection.application_context::<u32>().is_none());

        let test_value: u32 = 1142;
        connection.set_application_context(test_value);

        // After a context is set, the application data is returned.
        assert_eq!(*connection.application_context::<u32>().unwrap(), 1142);
    }

    /// Test that an application context can be modified.
    #[test]
    fn test_app_context_modify() {
        let test_value: u64 = 0;

        let mut connection = Connection::new_server();
        connection.set_application_context(test_value);

        let context_value = connection.application_context_mut::<u64>().unwrap();
        *context_value += 1;

        assert_eq!(*connection.application_context::<u64>().unwrap(), 1);
    }

    /// Test that an application context can be overridden.
    #[test]
    fn test_app_context_override() {
        let mut connection = Connection::new_server();

        let test_value: u16 = 1142;
        connection.set_application_context(test_value);

        assert_eq!(*connection.application_context::<u16>().unwrap(), 1142);

        // Override the context with a new value.
        let test_value: u16 = 10;
        connection.set_application_context(test_value);

        assert_eq!(*connection.application_context::<u16>().unwrap(), 10);

        // Override the context with a new type.
        let test_value: i16 = -20;
        connection.set_application_context(test_value);

        assert_eq!(*connection.application_context::<i16>().unwrap(), -20);
    }

    /// Test that a context of another type can't be retrieved.
    #[test]
    fn test_app_context_invalid_type() {
        let mut connection = Connection::new_server();

        let test_value: u32 = 0;
        connection.set_application_context(test_value);

        // A context type that wasn't set shouldn't be returned.
        assert!(connection.application_context::<i16>().is_none());

        // Retrieving the correct type succeeds.
        assert!(connection.application_context::<u32>().is_some());
    }
}
