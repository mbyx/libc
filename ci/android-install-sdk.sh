#!/usr/bin/env sh

set -eux

# Prep the SDK and emulator
#
# Note that the update process requires that we accept a bunch of licenses, and
# we can't just pipe `yes` into it for some reason, so we take the same strategy
# located in https://github.com/appunite/docker by just wrapping it in a script
# which apparently magically accepts the licenses.

sdk=6609375
mkdir -p sdk/cmdline-tools
wget -q --tries=20 "https://dl.google.com/android/repository/commandlinetools-linux-${sdk}_latest.zip"
unzip -q -d sdk/cmdline-tools "commandlinetools-linux-${sdk}_latest.zip"

case "$1" in
    arm | armv7)
        api=24
        image="system-images;android-${api};default;armeabi-v7a"
        ;;
    aarch64)
        api=24
        image="system-images;android-${api};google_apis;arm64-v8a"
        ;;
    i686)
        api=28
        image="system-images;android-${api};default;x86"
        ;;
    x86_64)
        api=28
        image="system-images;android-${api};default;x86_64"
        ;;
    *)
        echo "invalid arch: $1"
        exit 1
        ;;
esac

# Try to fix warning about missing file.
# See https://askubuntu.com/a/1078784
mkdir -p /root/.android/
echo '### User Sources for Android SDK Manager' >> /root/.android/repositories.cfg
echo '#Fri Nov 03 10:11:27 CET 2017 count=0' >> /root/.android/repositories.cfg

# Print all available packages
# yes | ./sdk/tools/bin/sdkmanager --list --verbose

# --no_https avoids
# javax.net.ssl.SSLHandshakeException: sun.security.validator.ValidatorException: No trusted certificate found
#
# | grep -v = || true    removes the progress bar output from the sdkmanager
# which produces an insane amount of output.
yes | ./sdk/cmdline-tools/tools/bin/sdkmanager --licenses --no_https | grep -v = || true
yes | ./sdk/cmdline-tools/tools/bin/sdkmanager --no_https \
    "platform-tools" \
    "platforms;android-${api}" \
    "${image}" | grep -v = || true

# The newer emulator versions (31.3.12 or higher) fail to a valid AVD and the test gets stuck.
# Until we figure out why, we use the older version (31.3.11).
wget -q --tries=20 https://redirector.gvt1.com/edgedl/android/repository/emulator-linux_x64-9058569.zip
unzip -q -d sdk emulator-linux_x64-9058569.zip

cp /android/android-emulator-package.xml /android/sdk/emulator/package.xml

echo "no" |
    ./sdk/cmdline-tools/tools/bin/avdmanager create avd \
        --name "${1}" \
        --package "${image}" | grep -v = || true

rm -rf "commandlinetools-linux-${sdk}_latest.zip" emulator-linux_x64-9058569.zip
