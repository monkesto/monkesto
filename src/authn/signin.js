function signin() {
    const challengeDataElement = document.getElementById('challenge-data');
    if (!challengeDataElement) {
        document.getElementById('flash_message').innerHTML = 'No challenge data available. Please refresh the page.';
        return;
    }

    let credentialRequestOptions;
    try {
        credentialRequestOptions = JSON.parse(challengeDataElement.textContent);
    } catch (error) {
        console.error('Failed to parse challenge data:', error);
        document.getElementById('flash_message').innerHTML = 'Invalid challenge data. Please refresh the page.';
        return;
    }

    // Convert base64url strings to Uint8Arrays
    credentialRequestOptions.publicKey.challenge = Base64.toUint8Array(
        credentialRequestOptions.publicKey.challenge
    );
    credentialRequestOptions.publicKey.allowCredentials?.forEach(function(listItem) {
        listItem.id = Base64.toUint8Array(listItem.id);
    });

    navigator.credentials.get({
        publicKey: credentialRequestOptions.publicKey
    }).then(function(assertion) {
        // Convert response to base64url and submit via form
        const credentialData = {
            id: assertion.id,
            rawId: Base64.fromUint8Array(new Uint8Array(assertion.rawId), true),
            type: assertion.type,
            response: {
                authenticatorData: Base64.fromUint8Array(new Uint8Array(assertion.response.authenticatorData), true),
                clientDataJSON: Base64.fromUint8Array(new Uint8Array(assertion.response.clientDataJSON), true),
                signature: Base64.fromUint8Array(new Uint8Array(assertion.response.signature), true),
                userHandle: Base64.fromUint8Array(new Uint8Array(assertion.response.userHandle), true)
            }
        };

        document.getElementById('credential-field').value = JSON.stringify(credentialData);
        document.getElementById('auth-form').submit();
    }).catch(function(error) {
        console.error('Authentication error:', error);
        document.getElementById('flash_message').innerHTML = 'Authentication failed: ' + error.message;
    });
}