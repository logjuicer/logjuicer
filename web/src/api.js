/* global process, window */
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

import Axios from 'axios'

function getHomepageUrl (url) {
  //
  // Discover serving location from href.
  //
  // This is only needed for sub-directory serving.
  // Serving the application from '/' may simply default to '/'
  //
  // Note that this is not enough for sub-directory serving,
  // The static files location also needs to be adapted with the 'homepage'
  // settings of the package.json file.
  //
  // This homepage url is used for the Router and Link resolution logic
  //
  let baseUrl
  if (url) {
    baseUrl = url
  } else {
    baseUrl = window.location.href
  }
  // Get dirname of the current url
  baseUrl = baseUrl.replace(/\\/g, '/').replace(/\/[^/]*$/, '/')

  // Remove any query strings
  if (baseUrl.includes('?')) {
    baseUrl = baseUrl.slice(0, baseUrl.lastIndexOf('?'))
  }
  // Remove any hash anchor
  if (baseUrl.includes('/#')) {
    baseUrl = baseUrl.slice(0, baseUrl.lastIndexOf('/#') + 1)
  }

  // Remove known sub-path
  const subDir = ['/view/']
  subDir.forEach(path => {
    if (baseUrl.includes(path)) {
      baseUrl = baseUrl.slice(0, baseUrl.lastIndexOf(path) + 1)
    }
  })

  if (! baseUrl.endsWith('/')) {
    baseUrl = baseUrl + '/'
  }
  // console.log('Homepage url is ', baseUrl)
  return baseUrl
}
function getZuulUrl () {
  // Return the zuul root api absolute url
  const LOG_CLASSIFY_API = process.env.REACT_APP_LOG_CLASSIFY_API
  let apiUrl

  if (LOG_CLASSIFY_API) {
    // Api url set at build time, use it
    apiUrl = LOG_CLASSIFY_API
  } else {
    // Api url is relative to homepage path
    apiUrl = getHomepageUrl () + 'api/'
  }
  if (! apiUrl.endsWith('/')) {
    apiUrl = apiUrl + '/'
  }
  if (! apiUrl.endsWith('/api/')) {
    apiUrl = apiUrl + 'api/'
  }
  // console.log('Api url is ', apiUrl)
  return apiUrl
}
const apiUrl = getZuulUrl()

// Direct APIs
function fetchStatus () {
  return Axios.get(apiUrl + 'status')
}
function fetchAnomalies () {
  return Axios.get(apiUrl + 'anomalies')
}
function fetchAnomaly (anomalyId) {
  return Axios.get(apiUrl + 'anomaly/' + anomalyId)
}
function deleteAnomaly (anomalyId) {
  return Axios.delete(apiUrl + 'anomaly/' + anomalyId)
}
function updateAnomaly (anomalyId, data) {
  return Axios.post(apiUrl + 'anomaly/' + anomalyId, data)
}
function removeLogfile (anomalyId, logfileId) {
  return Axios.delete(apiUrl + 'anomaly/' + anomalyId + '/logfile/' + logfileId)
}
function updateLogfile (anomalyId, logfileId, scores) {
  return Axios.post(
    apiUrl + 'anomaly/' + anomalyId + '/logfile/' + logfileId, scores)
}
function submitRequest (data) {
  return Axios.put(apiUrl + 'anomaly/new', data)
}


export {
  getHomepageUrl,
  fetchStatus,
  fetchAnomalies,
  fetchAnomaly,
  updateAnomaly,
  removeLogfile,
  updateLogfile,
  deleteAnomaly,
  submitRequest
}
