import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';

var typeaheads = document.getElementsByClassName('typeahead');

// console.log('TYPEAHEADS', typeaheads);
for (var i=0; i < typeaheads.length; i++) {
  const typeahead = typeaheads[i];
  if (typeahead.tagName.toLowerCase() !== "input") {
    continue;
  }
  // console.log('TYPEAHEAD', typeahead.id);
  const div = document.createElement("div");
  typeahead.setAttribute("type", "hidden");
  typeahead.parentNode.insertBefore(div, typeahead);
  const root = ReactDOM.createRoot(div);
  root.render(
    <React.StrictMode>
      <App
        id={typeahead.id}
        value={typeahead.getAttribute("value")}
        table={typeahead.getAttribute("data-table")}
        column={typeahead.getAttribute("data-column")}
        separator={typeahead.getAttribute("data-separator")}
        multiple={typeahead.classList.contains("multiple")}
        isValid={typeahead.classList.contains("is-valid")}/>
    </React.StrictMode>
  );
}
