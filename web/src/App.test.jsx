/* global it, Promise, jest */
// Copyright 2018 Red Hat, Inc
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

import React from 'react'
import ReactDOM from 'react-dom'
import { BrowserRouter as Router } from 'react-router-dom'
import { Provider } from 'react-redux'

import { createLogClassifyStore } from './reducers'
import App from './App'
import * as api from './api'

const fakeAnomalies = [
  {
    uuid: 'aaa',
    name: 'bbb',
    status: 'ccc',
    reporter: 'ddd',
    report_date: 'eee',
    build: {
      uuid: 'fff',
      pipeline: 'ggg',
      project: 'hhh',
      ref: 'iii'
    }
  }
]

api.fetchAnomalies = jest.fn().mockImplementation(
  () => Promise.resolve({data: fakeAnomalies}))

it('renders without crashing', () => {
  const div = document.createElement('div')
  const store = createLogClassifyStore()
  ReactDOM.render(<Provider store={store}><Router><App /></Router></Provider>,
    div)
  ReactDOM.unmountComponentAtNode(div)
})
