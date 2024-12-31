//! # Transparent proxy library for Linux and *BSD
//! Thank you for [shadowsocks-rust](https://github.com/shadowsocks/shadowsocks-rust), I referred to this.

mod tcp;

pub use tcp::{TcpListenerRedirExt, TcpStreamRedirExt};

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use cfg_if::cfg_if;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RedirType {
    /// For not supported platforms
    NotSupported,

    /// For Linux-like systems' Netfilter `REDIRECT`. Only for TCP connections.
    ///
    /// This is supported from Linux 2.4 Kernel. Document: <https://www.netfilter.org/documentation/index.html#documentation-howto>
    ///
    /// NOTE: Filter rule `REDIRECT` can only be applied to TCP connections.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    Redirect,

    /// For Linux-like systems' Netfilter TPROXY rule.
    ///
    /// NOTE: Filter rule `TPROXY` can be applied to TCP and UDP connections.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    TProxy,

    /// Packet Filter (pf)
    ///
    /// Supported by OpenBSD 3.0+, FreeBSD 5.3+, NetBSD 3.0+, Solaris 11.3+, macOS 10.7+, iOS, QNX
    ///
    /// Document: <https://www.freebsd.org/doc/handbook/firewalls-pf.html>
    #[cfg(any(
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "macos",
        target_os = "ios"
    ))]
    PacketFilter,

    /// IPFW
    ///
    /// Supported by FreeBSD, macOS 10.6- (Have been removed completely on macOS 10.10)
    ///
    /// Document: https://www.freebsd.org/doc/handbook/firewalls-ipfw.html
    #[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios"))]
    IpFirewall,
}

impl RedirType {
    cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "android"))] {
            /// Default TCP transparent proxy solution on this platform
            pub const fn tcp_default() -> RedirType {
                RedirType::Redirect
            }

            /// Available TCP transparent proxy types
            #[doc(hidden)]
            pub fn tcp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::Redirect.name(), RedirType::TProxy.name()];
                AVAILABLE_TYPES
            }

            /// Default UDP transparent proxy solution on this platform
            pub const fn udp_default() -> RedirType {
                RedirType::TProxy
            }

            /// Available UDP transparent proxy types
            #[doc(hidden)]
            pub fn udp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::TProxy.name()];
                AVAILABLE_TYPES
            }
        } else if #[cfg(any(target_os = "freebsd"))] {
            /// Default TCP transparent proxy solution on this platform
            pub fn tcp_default() -> RedirType {
                RedirType::PacketFilter
            }

            /// Available TCP transparent proxy types
            #[doc(hidden)]
            pub fn tcp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::PacketFilter.name(), RedirType::IpFirewall.name()];
                AVAILABLE_TYPES
            }

            /// Default UDP transparent proxy solution on this platform
            pub fn udp_default() -> RedirType {
                RedirType::PacketFilter
            }

            /// Available UDP transparent proxy types
            #[doc(hidden)]
            pub const fn udp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::PacketFilter.name(), RedirType::IpFirewall.name()];
                AVAILABLE_TYPES
            }
        } else if #[cfg(target_os = "openbsd")] {
            /// Default TCP transparent proxy solution on this platform
            pub fn tcp_default() -> RedirType {
                RedirType::PacketFilter
            }

            /// Available TCP transparent proxy types
            #[doc(hidden)]
            pub fn tcp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::PacketFilter.name()];
                AVAILABLE_TYPES
            }

            /// Default UDP transparent proxy solution on this platform
            pub fn udp_default() -> RedirType {
                RedirType::PacketFilter
            }

            /// Available UDP transparent proxy types
            #[doc(hidden)]
            pub const fn udp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::PacketFilter.name()];
                AVAILABLE_TYPES
            }
        } else if #[cfg(any(target_os = "macos", target_os = "ios"))] {
            /// Default TCP transparent proxy solution on this platform
            pub fn tcp_default() -> RedirType {
                RedirType::PacketFilter
            }

            /// Available TCP transparent proxy types
            #[doc(hidden)]
            pub const fn tcp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::PacketFilter.name(), RedirType::IpFirewall.name()];
                AVAILABLE_TYPES
            }

            /// Default UDP transparent proxy solution on this platform
            pub fn udp_default() -> RedirType {
                RedirType::PacketFilter
            }

            /// Available UDP transparent proxy types
            #[doc(hidden)]
            pub const fn udp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[RedirType::PacketFilter.name()];
                AVAILABLE_TYPES
            }
        } else {
            /// Default TCP transparent proxy solution on this platform
            pub fn tcp_default() -> RedirType {
                RedirType::NotSupported
            }

            /// Available TCP transparent proxy types
            #[doc(hidden)]
            pub const fn tcp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[];
                AVAILABLE_TYPES
            }

            /// Default UDP transparent proxy solution on this platform
            pub fn udp_default() -> RedirType {
                RedirType::NotSupported
            }

            /// Available UDP transparent proxy types
            #[doc(hidden)]
            pub const fn udp_available_types() -> &'static [&'static str] {
                const AVAILABLE_TYPES: &[&str] = &[];
                AVAILABLE_TYPES
            }
        }
    }

    /// Check if transparent proxy is supported on this platform
    pub fn is_supported(self) -> bool {
        self != RedirType::NotSupported
    }

    /// Name of redirect type (transparent proxy type)
    pub const fn name(self) -> &'static str {
        match self {
            // Dummy, shouldn't be used in any useful situations
            RedirType::NotSupported => "not_supported",

            #[cfg(any(target_os = "linux", target_os = "android"))]
            RedirType::Redirect => "redirect",

            #[cfg(any(target_os = "linux", target_os = "android"))]
            RedirType::TProxy => "tproxy",

            #[cfg(any(
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "macos",
                target_os = "ios"
            ))]
            RedirType::PacketFilter => "pf",

            #[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios"))]
            RedirType::IpFirewall => "ipfw",
        }
    }
}

impl Display for RedirType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Error type for `RedirType`'s `FromStr::Err`
#[derive(Debug)]
pub struct InvalidRedirType;

impl Display for InvalidRedirType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("invalid RedirType")
    }
}

impl std::error::Error for InvalidRedirType {}

impl FromStr for RedirType {
    type Err = InvalidRedirType;

    fn from_str(s: &str) -> Result<RedirType, InvalidRedirType> {
        match s {
            #[cfg(any(target_os = "linux", target_os = "android"))]
            "redirect" => Ok(RedirType::Redirect),

            #[cfg(any(target_os = "linux", target_os = "android"))]
            "tproxy" => Ok(RedirType::TProxy),

            #[cfg(any(
                target_os = "freebsd",
                target_os = "openbsd",
                target_os = "macos",
                target_os = "ios",
            ))]
            "pf" => Ok(RedirType::PacketFilter),

            #[cfg(any(target_os = "freebsd", target_os = "macos", target_os = "ios",))]
            "ipfw" => Ok(RedirType::IpFirewall),

            _ => Err(InvalidRedirType),
        }
    }
}
