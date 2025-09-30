# AppArmor profile for Docker containers launched by aifo-coder
# Based on Docker's default (docker-default) profile with sane allowances
# for typical developer workloads. It aims to preserve container isolation
# without hindering normal agent operations (network, file IO, etc).

#include <tunables/global>

profile __PROFILE_NAME__ flags=(attach_disconnected,mediate_deleted) {
  # Base abstractions
  # (nameservice for DNS, user-tmp for tmp usage, openssl for crypto libs)
  # Keep these minimal to avoid over-broad grants.
  # If you need more, extend this template as required for your environment.
  #include <abstractions/base>
  #include <abstractions/nameservice>
  #include <abstractions/user-tmp>
  #include <abstractions/openssl>

  # Allow normal networking and file operations in the container
  network,
  capability,
  file,

  # Allow umount inside container namespace but block mount (like docker-default)
  umount,
  deny mount,

  # Hardening: block access to sensitive host kernel interfaces
  deny /sys/kernel/security/** rwklx,
  deny /proc/sysrq-trigger rwklx,
  deny /proc/kcore rwklx,
  deny /proc/keys rwklx,
  deny /proc/lock rwklx,
  deny /proc/tty/** rwklx,
  deny /proc/*/mem rwklx,
  deny /sys/firmware/** rwklx,

  # Avoid ptrace across boundaries
  deny ptrace (readby, tracedby, trace),

  # Devices commonly needed by CLI tools
  /dev/null rw,
  /dev/zero r,
  /dev/urandom r,
  /dev/random r,
  /dev/tty rw,
  /dev/pts/[0-9]* rw,
  /dev/shm/** rwk,
  deny /dev/mem rwklx,
  deny /dev/kmem rwklx,
  deny /dev/kmsg rwklx,

  # Permit reading system configuration required for DNS and NSS
  /etc/hosts r,
  /etc/resolv.conf r,
  /etc/nsswitch.conf r,
  /etc/** r,

  # Allow execution of binaries and interpreters
  /bin/** rix,
  /sbin/** rix,
  /usr/bin/** rix,
  /usr/sbin/** rix,
  /usr/local/bin/** rix,
  /usr/local/sbin/** rix,
  /opt/aifo/bin/** rix,
  /opt/venv/** rixm,
  /opt/venv-openhands/** rixm,
  # Git plumbing and shared data reads required by pinentry/gpg/git
  /usr/lib/git-core/** rix,
  /usr/libexec/git-core/** rix,
  /usr/share/** r,

  # Allow mapping of shared libraries
  /lib/** mr,
  /lib64/** mr,
  /usr/lib/** mr,
  /usr/lib64/** mr,
  /usr/local/lib/** mr,

  # Writable work areas (working tree, HOME, temp)
  /workspace/** rwkmla,
  /home/coder/** rwkml,
  /tmp/** rwkml,
  /var/tmp/** rwkml,
  /var/log/host/** r,

  # Explicit allowances for aifo-coder lock files (workspace-wide .git is already covered by /workspace/**)
  /workspace/.aifo-coder.lock rwkml,
  /home/coder/.aifo-coder.lock rwkml,
  /tmp/aifo-coder.lock rwkml,
  /run/user/[0-9]*/aifo-coder.lock rwkml,

  # Allow gpg-agent sockets if it chooses systemd-style runtime dir
  /run/user/[0-9]*/gnupg/** rwkml,
  # And if we fall back to a private runtime dir inside /tmp
  /tmp/runtime-[0-9]*/gnupg/** rwkml,

  # Deny writes to system areas inside the container rootfs
  deny /bin/** wklx,
  deny /sbin/** wklx,
  deny /usr/** wklx,
  deny /lib/** wklx,
  deny /lib64/** wklx,
  deny /opt/** wklx,

  # Allow limited /proc reads; block sensitive interfaces and sysctls
  /proc/cpuinfo r,
  /proc/meminfo r,
  /proc/self/** r,
  /proc/self/fd/** r,
  /proc/[0-9]*/fd/** r,
  /proc/[0-9]*/status r,
  deny /proc/sys/** rwklx,
  deny /proc/[0-9]*/task/** rwklx,

  # Block sysfs and cgroup manipulation
  deny /sys/** rwklx,
  deny /sys/fs/cgroup/** rwklx,

  # Tighten capabilities beyond Docker defaults
  deny capability sys_module,
  deny capability sys_admin,
  deny capability sys_ptrace,
  deny capability sys_time,
  deny capability sys_boot,
  deny capability sys_rawio,
  deny capability mknod,
}
