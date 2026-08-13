async function upload() {
    const progressBar = document.getElementById('progressBar');
    const progressContainer = document.getElementById('progressContainer');
    const statusText = document.getElementById("statusText")

    const input = document.createElement('input');
    input.type = 'file';

    input.onchange = e => {
        let file = e.target.files[0];
        if (!file) return;

        let reader = new FileReader();
        reader.readAsArrayBuffer(file);

        reader.onload = async readerEvent => {
            const res = await fetch(window.location.href + "/upload", {
                method: 'POST',
                headers: {'Content-Type': 'application/json'},
                body: JSON.stringify({file_name: file.name, file_size: file.size}),
            })

            if (res.redirected) {
                window.location.replace(res.url)
            }

            const {upload_url, file_key} = await res.json();

            const xhr = new XMLHttpRequest();
            xhr.open('PUT', upload_url, true);
            xhr.setRequestHeader('Content-Type', file.type || 'application/octet-stream');

            progressContainer.classList.remove('hidden');

            xhr.upload.onprogress = (event) => {
                if (event.lengthComputable) {
                    const percentComplete = Math.round((event.loaded / event.total) * 100);
                    progressBar.style.width = `${percentComplete}%`;
                    statusText.innerText = `Uploading: ${percentComplete}%`;
                }
            }

            xhr.onload = async () => {
                if (xhr.status === 200 || xhr.status === 204) {
                    progressContainer.classList.add('hidden');
                    statusText.innerText = "";

                    const res = await fetch(window.location.href + "/recordupload", {
                        method: 'POST',
                        headers: {'Content-Type': 'application/json'},
                        body: JSON.stringify({file_key: file_key}),
                    })

                    window.location.replace(res.url)

                } else {
                    console.log("upload failed: " + xhr.status)
                }
            }

            xhr.send(file);

        }
    }

    input.click();

}

