import { AsyncTypeahead, Token } from 'react-bootstrap-typeahead';
import { useState } from 'react';
import './App.css';
import 'bootstrap/dist/css/bootstrap.css';
import 'react-bootstrap-typeahead/css/Typeahead.css';

function App() {
  const [isLoading, setIsLoading] = useState(false);
  const [options, setOptions] = useState([]);
  const handleSearch = (query: string) => {
    setIsLoading(true);
    // console.log("FETCH", `/table?text=${query}&column=type&format=json`);
    fetch(`/table?text=${query}&column=type&format=json`)
      .then((resp) => resp.json())
      .then((items) => {
        // console.log("ITEMS", items);
        setOptions(items);
        setIsLoading(false);
      });
  };
  return (
    <div className="App">
      <AsyncTypeahead
        isLoading={isLoading}
        onChange={(selected) => {
          // console.log("SELECTED", selected);
          var values = selected.map((item) => item.id);
          // console.log("VALUES", values);
          var value = values.join(" ");
          // console.log("VALUE", value);
          document.getElementById("root-value").value = value;
        }}
        onSearch={handleSearch}
        options={options}
        multiple="true"
      />
    </div>
  );
}

export default App;
