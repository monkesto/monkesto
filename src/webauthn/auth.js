function register() {
  let username = document.getElementById("username").value;
  if (username === "") {
    alert("Please enter a username");
    return;
  }

  const baseUrl = document
    .querySelector('meta[name="webauthn_url"]')
    .getAttribute("content");

  console.log("Starting registration for username:", username);
  console.log("Base URL:", baseUrl);

  fetch(baseUrl + "register_start/" + encodeURIComponent(username), {
    method: "POST",
  })
    .then((response) => {
      console.log("Register start response status:", response.status);
      if (!response.ok) {
        throw new Error(`Register start failed: ${response.status}`);
      }
      return response.json();
    })
    .then((credentialCreationOptions) => {
      console.log(
        "Received credential creation options:",
        credentialCreationOptions,
      );

      credentialCreationOptions.publicKey.challenge = Base64.toUint8Array(
        credentialCreationOptions.publicKey.challenge,
      );
      credentialCreationOptions.publicKey.user.id = Base64.toUint8Array(
        credentialCreationOptions.publicKey.user.id,
      );
      credentialCreationOptions.publicKey.excludeCredentials?.forEach(
        function (listItem) {
          listItem.id = Base64.toUint8Array(listItem.id);
        },
      );

      console.log("Calling navigator.credentials.create...");
      return navigator.credentials.create({
        publicKey: credentialCreationOptions.publicKey,
      });
    })
    .then((credential) => {
      console.log("Credential created successfully:", credential);

      fetch(baseUrl + "register_finish", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          id: credential.id,
          rawId: Base64.fromUint8Array(new Uint8Array(credential.rawId), true),
          type: credential.type,
          response: {
            attestationObject: Base64.fromUint8Array(
              new Uint8Array(credential.response.attestationObject),
              true,
            ),
            clientDataJSON: Base64.fromUint8Array(
              new Uint8Array(credential.response.clientDataJSON),
              true,
            ),
          },
        }),
      }).then((response) => {
        console.log("Register finish response status:", response.status);
        const flash_message = document.getElementById("flash_message");
        if (response.ok) {
          flash_message.innerHTML = "Successfully registered!";
        } else {
          flash_message.innerHTML = "Error whilst registering!";
        }
      });
    })
    .catch((error) => {
      console.error("Registration error:", error);
      const flash_message = document.getElementById("flash_message");
      flash_message.innerHTML = `Registration failed: ${error.message}`;
    });
}

function login() {
  const baseUrl = document
    .querySelector('meta[name="webauthn_url"]')
    .getAttribute("content");

  fetch(baseUrl + "login_start", {
    method: "POST",
  })
    .then((response) => response.json())
    .then((credentialRequestOptions) => {
      credentialRequestOptions.publicKey.challenge = Base64.toUint8Array(
        credentialRequestOptions.publicKey.challenge,
      );
      credentialRequestOptions.publicKey.allowCredentials?.forEach(
        function (listItem) {
          listItem.id = Base64.toUint8Array(listItem.id);
        },
      );

      return navigator.credentials.get({
        publicKey: credentialRequestOptions.publicKey,
      });
    })
    .then((assertion) => {
      fetch(baseUrl + "login_finish", {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          id: assertion.id,
          rawId: Base64.fromUint8Array(new Uint8Array(assertion.rawId), true),
          type: assertion.type,
          response: {
            authenticatorData: Base64.fromUint8Array(
              new Uint8Array(assertion.response.authenticatorData),
              true,
            ),
            clientDataJSON: Base64.fromUint8Array(
              new Uint8Array(assertion.response.clientDataJSON),
              true,
            ),
            signature: Base64.fromUint8Array(
              new Uint8Array(assertion.response.signature),
              true,
            ),
            userHandle: Base64.fromUint8Array(
              new Uint8Array(assertion.response.userHandle),
              true,
            ),
          },
        }),
      }).then((response) => {
        const flash_message = document.getElementById("flash_message");
        if (response.ok) {
          flash_message.innerHTML = "Successfully logged in!";
        } else {
          flash_message.innerHTML = "Error whilst logging in!";
        }
      });
    });
}
