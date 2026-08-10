window.addEventListener('load', function() {
    const challengeDataElement = document.getElementById('challenge-data');
    if (!challengeDataElement) {
        document.getElementById('flash_message').innerHTML = 'No challenge data available. Please try again.';
        return;
    }

    let credentialCreationOptions;
    try {
        credentialCreationOptions = JSON.parse(challengeDataElement.textContent);
    } catch (error) {
        console.error('Failed to parse challenge data:', error);
        document.getElementById('flash_message').innerHTML = 'Invalid challenge data. Please try again.';
        return;
    }

    // Convert base64url strings to Uint8Arrays
    credentialCreationOptions.publicKey.challenge = Base64.toUint8Array(
        credentialCreationOptions.publicKey.challenge
    );
    credentialCreationOptions.publicKey.user.id = Base64.toUint8Array(
        credentialCreationOptions.publicKey.user.id
    );
    credentialCreationOptions.publicKey.excludeCredentials?.forEach(function(listItem) {
        listItem.id = Base64.toUint8Array(listItem.id);
    });

    // Show creating message
    document.getElementById('status_message').innerHTML = 'Creating your passkey...';

    navigator.credentials.create({
        publicKey: credentialCreationOptions.publicKey
    }).then(function(credential) {
        // Convert response to base64url and submit via form
        const credentialData = {
            id: credential.id,
            rawId: Base64.fromUint8Array(new Uint8Array(credential.rawId), true),
            type: credential.type,
            response: {
                attestationObject: Base64.fromUint8Array(
                    new Uint8Array(credential.response.attestationObject), true
                ),
                clientDataJSON: Base64.fromUint8Array(
                    new Uint8Array(credential.response.clientDataJSON), true
                )
            }
        };

        document.getElementById('credential-field').value = JSON.stringify(credentialData);
        document.getElementById('registration-form').submit();
    }).catch(function(error) {
        console.error('Registration error:', error);
        document.getElementById('flash_message').innerHTML = 'Failed to create passkey: ' + error.message;
    });
});