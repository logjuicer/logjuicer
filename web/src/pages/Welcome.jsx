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

import AnomalyList from './List'

class WelcomePage extends React.Component {
  render () {
    return (
      <React.Fragment>
        <p>{'Welcome. This service host an anomaly database.'}
           {'Use the "New" button to request a new report.'}</p>
        <p>Below is the list of reported and archived anomalies.
           Click on the name to see the details.</p>
        <AnomalyList />
      </React.Fragment>
    )
  }
}

export default WelcomePage
