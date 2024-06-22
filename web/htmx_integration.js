

document.body.addEventListener("navigator", function(evt){
    console.log(`navigator event ${evt.detail}`);
    if (evt.detail === undefined || evt.detail === null) {
        return;
    }

    const navigatorElement = document.getElementById("navigator");

    if (navigatorElement) {
        console.log(`FOUND navigator ${navigatorElement}`);
        // Find all <a> children under the navigator element
        const links = navigatorElement.getElementsByTagName("a");
    
        // Iterate over all found <a> elements
        for (let link of links) {
            console.log(`${link.href} === ${evt.detail.route}`);
            // Check if the href value equals x
            const linkPath = new URL(link.href).pathname;
            if (linkPath === evt.detail.route) {
                console.log(`MATCHED ${evt.detail}`);
                // Add the class if href equals x

                // todo -- the header data should drive these classes!
                link.classList.add("bg-gray-400");
                link.classList.remove("bg-gray-600");
            } else {
                // Remove the class if href does not equal x
                
                link.classList.add("bg-gray-600");
                link.classList.remove("bg-gray-400");
            }
        }
    }
})