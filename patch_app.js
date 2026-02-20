const fs = require('fs');
const file = './src/App.tsx';
let data = fs.readFileSync(file, 'utf8');
const lines = data.split('\n');

// Find the rogue modal at the very end
let modalStartIndex = -1;
let modalEndIndex = -1;
for (let i = lines.length - 1; i >= 0; i--) {
    if (lines[i].includes('{/* Import Platform Modal */}')) {
        modalStartIndex = i;
    }
    if (modalStartIndex !== -1 && lines[i] === '}') {
        modalEndIndex = i - 1; // Just before the final closing brace
        break;
    }
}

if (modalStartIndex !== -1 && modalEndIndex !== -1) {
    // Extract the modal lines
    const modalLines = lines.splice(modalStartIndex, modalEndIndex - modalStartIndex + 1);
    
    // Find where the App component returns its main div (just before the closing `)` and `}`)
    let injectionIndex = -1;
    for (let i = lines.length - 1; i >= 0; i--) {
        if (lines[i] === '  </div>') {
            injectionIndex = i;
            break;
        }
    }
    
    if (injectionIndex !== -1) {
        // Inject the modal lines before `  </div>`
        lines.splice(injectionIndex, 0, ...modalLines);
        fs.writeFileSync(file, lines.join('\n'));
        console.log("Successfully patched App.tsx");
    } else {
        console.log("Could not find injection point `  </div>`");
    }
} else {
    console.log("Could not find the rogue modal at the end of the file");
}
