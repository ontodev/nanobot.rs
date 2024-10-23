import { AsyncTypeahead } from 'react-bootstrap-typeahead';
import { createRef, useState } from 'react';
import './App.css';
import 'bootstrap/dist/css/bootstrap.css';
import 'react-bootstrap-typeahead/css/Typeahead.css';

function App(args) {
  // console.log("Starting App for", args, args.id, args.table, args.column);
  const [isLoading, setIsLoading] = useState(false);
  const [options, setOptions] = useState([]);

  const handleSearch = (query) => {
    // console.log("Starting search for", query);
    setIsLoading(true);
    const url = `../../${args.table}?text=${query}&column=${args.column}&format=json`;
    // console.log("URL", url);
    fetch(url)
      .then((resp) => resp.json())
      .then((items) => {
        setOptions(items);
        setIsLoading(false);
      });
  };
  // console.log("Starting App for", args, args.id, args.table, args.column);
  const ref = createRef();
  var value = args.value;
  var selected = [{"id": args.value, "label": args.value, "order": 1}];
  if (args.multiple) {
    value = "";
    selected = args.value.trim().split(args.separator).filter((item) => {
        return item.trim() !== "";
      }).map((item, order) => {
        return {"id": item, "label": item, "order": order}
      });
  }
  return (
    <div className="App">
      <AsyncTypeahead
        ref={ref}
        inputProps={{"name": args.name}}
        minLength={0}
        multiple={args.multiple}
        isLoading={isLoading}
        isValid={args.isValid}
        isInvalid={!args.isValid}
        onChange={(selected) => {
          // Set value of original input element to selected value.
          var values = selected.map((item) => item.id);
          var value = values.join(args.separator).trim();
          document.getElementById(args.id).value = value;
          if (value === "") {
            handleSearch("");
          }
        }}
        onFocus={(event) => {
          // Search for current values.
          handleSearch(event.target.value);
        }}
        onBlur={(event) => {
          // Set value of original input element to an invalid/incomplete value.
          if (!args.multiple) {
            document.getElementById(args.id).value = event.target.value;
          }
        }}
        onSearch={handleSearch}
        defaultInputValue={value}
        defaultSelected={selected}
        options={options}
      />
    </div>
  );
}

export default App;
