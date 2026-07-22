# Preview APK signing

`slot-manager-app` release APKs use one stable signing identity so a later GitHub Actions build can update an earlier fixed-signed build.

The private PKCS#12 keystore and passwords are never committed. They are stored as repository Actions secrets:

```text
RELEASE_KEYSTORE_BASE64
RELEASE_STORE_PASSWORD
RELEASE_KEY_ALIAS
RELEASE_KEY_PASSWORD
```

The workflow decodes the keystore only into the temporary runner directory. The repository contains the public certificate and its expected SHA-256 digest so every produced APK can be checked against the fixed identity before upload.

Files:

```text
uclone-slices-preview-cert.pem
UCLONE_SLICES_PREVIEW_CERT_SHA256
```

Installing a fixed-signed release over an older debug-signed APK will fail because Android sees a different signing identity. Uninstall the debug build once, install the fixed-signed APK, and subsequent fixed-signed builds can update it in place as long as the application ID remains `com.uclone.slots.preview`.
